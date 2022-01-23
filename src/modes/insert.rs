use std::borrow::Cow;
use std::collections::HashMap;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use lazy_static::lazy_static;

use crate::keymap::KeyMap;
use crate::modes::{
    mode::{Mode, ModeTransition},
    normal::Normal,
};
use crate::operations as ops;
use crate::selection::Direction;
use crate::{Buffer, Buffers};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum InsertionMode {
    Insert,
    Append,
    Overwrite,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Insert {
    pub mode: InsertionMode,
    pub hex: bool,
    pub hex_half: Option<u8>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Action {
    InsertNull,
    SwitchInputMode,
    RemoveLast,
    RemoveThis,
    Exit,
    Move(Direction),
}

fn default_maps() -> KeyMap<Action> {
    KeyMap {
        maps: keys!(
            (ctrl 'n' => Action::InsertNull),
            (ctrl 'o' => Action::SwitchInputMode),
            (key KeyCode::Backspace => Action::RemoveLast),
            (key KeyCode::Delete => Action::RemoveThis),
            (key KeyCode::Esc => Action::Exit),
            (key KeyCode::Right => Action::Move(Direction::Right)),
            (key KeyCode::Left => Action::Move(Direction::Left)),
            (key KeyCode::Up => Action::Move(Direction::Up)),
            (key KeyCode::Down => Action::Move(Direction::Down))
        ),
    }
}

lazy_static! {
    static ref DEFAULT_MAPS: KeyMap<Action> = default_maps();
}

fn transition_ascii_insertion(key: char, buffer: &mut Buffer, mode: InsertionMode) -> ModeTransition {
    let mut inserted_bytes = vec![0u8; key.len_utf8()];
    key.encode_utf8(&mut inserted_bytes);

    match mode {
        InsertionMode::Append | InsertionMode::Insert => {
            let delta = ops::insert(&buffer.data, &buffer.selection, inserted_bytes);
            ModeTransition::DirtyBytes(buffer.apply_incomplete_delta(delta))
        }
        InsertionMode::Overwrite => {
            let delta = ops::change(&buffer.data, &buffer.selection, inserted_bytes);
            ModeTransition::DirtyBytes(buffer.apply_incomplete_delta(delta))
        }
    }
}

fn transition_hex_insertion(
    key: char,
    buffer: &mut Buffer,
    mode: InsertionMode,
    hex_half: Option<u8>,
) -> Option<ModeTransition> {
    if !key.is_ascii_hexdigit() {
        return None;
    }

    let digit = key.to_digit(16).unwrap() as u8;
    let to_insert = hex_half.map(|x| x | digit).unwrap_or(digit << 4);
    let insert_half = hex_half.is_none();

    if insert_half {
        match mode {
            InsertionMode::Append | InsertionMode::Insert => {
                let delta = ops::insert(&buffer.data, &buffer.selection, vec![to_insert]);
                Some(ModeTransition::new_mode_and_dirty(
                    Insert {
                        mode,
                        hex: true,
                        hex_half: Some(to_insert),
                    },
                    buffer.apply_incomplete_delta_offset_carets(delta, -1, 0),
                ))
            }
            InsertionMode::Overwrite => {
                let delta = ops::overwrite_half(&buffer.data, &buffer.selection, to_insert);
                Some(ModeTransition::new_mode_and_dirty(
                    Insert {
                        mode,
                        hex: true,
                        hex_half: Some(to_insert),
                    },
                    buffer.apply_incomplete_delta_offset_carets(delta, -1, 0),
                ))
            }
        }
    } else {
        let delta = ops::change(&buffer.data, &buffer.selection, vec![to_insert]);
        Some(ModeTransition::new_mode_and_dirty(
            Insert {
                mode,
                hex: true,
                hex_half: None,
            },
            buffer.apply_incomplete_delta(delta),
        ))
    }
}

impl Mode for Insert {
    fn name(&self) -> Cow<'static, str> {
        match (self.mode, self.hex) {
            (InsertionMode::Insert, true) => "INSERT (hex)".into(),
            (InsertionMode::Insert, false) => "INSERT (ascii)".into(),
            (InsertionMode::Append, true) => "APPEND (hex)".into(),
            (InsertionMode::Append, false) => "APPEND (ascii)".into(),
            (InsertionMode::Overwrite, true) => "OVERWRITE (hex)".into(),
            (InsertionMode::Overwrite, false) => "OVERWRITE (ascii)".into(),
        }
    }

    fn has_half_cursor(&self) -> bool {
        self.hex_half.is_some()
    }

    fn transition(
        &self,
        evt: &Event,
        buffers: &mut Buffers,
        bytes_per_line: usize,
    ) -> Option<ModeTransition> {
        let buffer = buffers.current_mut();
        if let Some(action) = DEFAULT_MAPS.event_to_action(evt) {
            let new_state = if self.hex_half.is_some() {
                Insert {
                    hex_half: None,
                    ..*self
                }
            } else {
                *self
            };
            Some(match action {
                Action::Exit => {
                    buffer.commit_delta(); // Flush this insertion as a single action
                    ModeTransition::new_mode(Normal::new())
                }
                Action::InsertNull => {
                    let inserted_bytes = vec![0];
                    let delta = ops::insert(&buffer.data, &buffer.selection, inserted_bytes);
                    ModeTransition::new_mode_and_dirty(
                        new_state,
                        buffer.apply_incomplete_delta(delta),
                    )
                }
                Action::SwitchInputMode => ModeTransition::new_mode(Insert {
                    mode: self.mode,
                    hex: !self.hex,
                    hex_half: None,
                }),
                Action::RemoveLast | Action::RemoveThis if self.hex_half.is_some() => {
                    if buffer.data.is_empty() {
                        return Some(ModeTransition::None);
                    }
                    let delta = ops::delete_cursor(&buffer.data, &buffer.selection);
                    ModeTransition::new_mode_and_dirty(
                        new_state,
                        buffer.apply_incomplete_delta(delta),
                    )
                }
                Action::RemoveLast => {
                    if buffer.data.is_empty() {
                        return Some(ModeTransition::None);
                    }
                    let delta = ops::backspace(&buffer.data, &buffer.selection);
                    ModeTransition::DirtyBytes(buffer.apply_incomplete_delta(delta))
                }
                Action::RemoveThis => {
                    if buffer.data.is_empty() {
                        return Some(ModeTransition::None);
                    }
                    let delta = ops::delete_cursor(&buffer.data, &buffer.selection);
                    ModeTransition::DirtyBytes(buffer.apply_incomplete_delta(delta))
                }
                Action::Move(direction) => {
                    let is_hex_half = self.hex_half.is_some();
                    if is_hex_half {
                        transition_hex_insertion('0', buffer, self.mode, self.hex_half);
                    }
                    let max_bytes = buffer.data.len();
                    ModeTransition::new_mode_and_dirty(
                        Insert {
                            mode: self.mode,
                            hex: self.hex,
                            hex_half: None,
                        },
                        buffer.map_selections(|region| {
                            let mut region =
                                region.simple_move(direction, bytes_per_line, max_bytes, 1);
                            if is_hex_half {
                                region = region.simple_move(
                                    Direction::Left,
                                    bytes_per_line,
                                    max_bytes,
                                    1,
                                );
                            }
                            vec![region]
                        }),
                    )
                }
            })
        } else if let Event::Key(KeyEvent {
            code: KeyCode::Char(key),
            modifiers,
        }) = evt
        {
            if !(*modifiers & !KeyModifiers::SHIFT).is_empty() {
                return None;
            }

            if self.hex {
                transition_hex_insertion(*key, buffer, self.mode, self.hex_half)
            } else {
                Some(transition_ascii_insertion(*key, buffer, self.mode))
            }
        } else {
            None
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
