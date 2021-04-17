use std::borrow::Cow;
use std::collections::HashMap;

use super::buffer::*;
use super::keymap::*;
use super::mode::*;
use super::modes::normal::Normal;
use super::operations as ops;
use super::view::view_options::ViewOptions;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use lazy_static::lazy_static;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Insert {
    pub before: bool,
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
}

fn default_maps() -> KeyMap<Action> {
    KeyMap {
        maps: keys!(
            (ctrl 'n' => Action::InsertNull),
            (ctrl 'o' => Action::SwitchInputMode),
            (key KeyCode::Backspace => Action::RemoveLast),
            (key KeyCode::Delete => Action::RemoveThis),
            (key KeyCode::Esc => Action::Exit)
        ),
    }
}

lazy_static! {
    static ref DEFAULT_MAPS: KeyMap<Action> = default_maps();
}

fn transition_ascii_insertion(
    mode: Insert,
    key: char,
    buffer: &mut Buffer,
    options: &mut ViewOptions,
) -> ModeTransition {
    let mut inserted_bytes = vec![0u8; key.len_utf8()];
    key.encode_utf8(&mut inserted_bytes);

    // At this point `before` doesn't really matter;
    // the cursors will have been moved in normal mode to their
    // correct places.
    let delta = ops::insert(&buffer.data, &buffer.selection, inserted_bytes);
    options.make_dirty(buffer.apply_incomplete_delta(delta));
    ModeTransition::new_mode(mode)
}

fn transition_hex_insertion(
    mode: Insert,
    key: char,
    buffer: &mut Buffer,
    options: &mut ViewOptions,
    before: bool,
    hex_half: Option<u8>,
) -> ModeTransition {
    if !key.is_ascii_hexdigit() {
        return ModeTransition::not_handled(mode);
    }

    let digit = key.to_digit(16).unwrap() as u8;
    let to_insert = hex_half.map(|x| x | digit).unwrap_or(digit << 4);
    let insert_half = hex_half.is_none();

    if insert_half {
        let delta = ops::insert(&buffer.data, &buffer.selection, vec![to_insert]);
        options.make_dirty(buffer.apply_incomplete_delta_offset_carets(delta, -1, 0));
        ModeTransition::new_mode(Insert {
            before,
            hex: true,
            hex_half: Some(to_insert),
        })
    } else {
        let delta = ops::change(&buffer.data, &buffer.selection, vec![to_insert]);
        options.make_dirty(buffer.apply_incomplete_delta(delta));
        ModeTransition::new_mode(Insert {
            before,
            hex: true,
            hex_half: None,
        })
    }
}

impl Mode for Insert {
    fn name(&self) -> Cow<'static, str> {
        match (self.before, self.hex) {
            (true, true) => "INSERT (hex)".into(),
            (true, false) => "INSERT (ascii)".into(),
            (false, true) => "APPEND (hex)".into(),
            (false, false) => "APPEND (ascii)".into(),
        }
    }
    fn has_half_cursor(&self) -> bool {
        self.hex_half.is_some()
    }
    fn transition(
        self,
        evt: &Event,
        buffers: &mut Buffers,
        options: &mut ViewOptions,
    ) -> ModeTransition {
        let buffer = buffers.current_mut();
        if let Some(action) = DEFAULT_MAPS.event_to_action(evt) {
            let new_state = if self.hex_half.is_some() {
                Insert {
                    before: self.before,
                    hex: self.hex,
                    hex_half: None,
                }
            } else {
                self
            };
            match action {
                Action::Exit => {
                    buffer.commit_delta(); // Flush this insertion as a single action
                    ModeTransition::new_mode(Normal::new())
                }
                Action::InsertNull => {
                    let inserted_bytes = vec![0];
                    let delta = ops::insert(&buffer.data, &buffer.selection, inserted_bytes);
                    options.make_dirty(buffer.apply_incomplete_delta(delta));
                    ModeTransition::new_mode(new_state)
                }
                Action::SwitchInputMode => ModeTransition::new_mode(Insert {
                    before: self.before,
                    hex: !self.hex,
                    hex_half: None,
                }),
                Action::RemoveLast | Action::RemoveThis if self.hex_half.is_some() => {
                    if buffer.data.is_empty() {
                        return ModeTransition::new_mode(self);
                    }
                    let delta = ops::delete_cursor(&buffer.data, &buffer.selection);
                    options.make_dirty(buffer.apply_incomplete_delta(delta));
                    ModeTransition::new_mode(new_state)
                }
                Action::RemoveLast => {
                    if buffer.data.is_empty() {
                        return ModeTransition::new_mode(self);
                    }
                    let delta = ops::backspace(&buffer.data, &buffer.selection);
                    options.make_dirty(buffer.apply_incomplete_delta(delta));
                    ModeTransition::new_mode(self)
                }
                Action::RemoveThis => {
                    if buffer.data.is_empty() {
                        return ModeTransition::new_mode(self);
                    }
                    let delta = ops::delete_cursor(&buffer.data, &buffer.selection);
                    options.make_dirty(buffer.apply_incomplete_delta(delta));
                    ModeTransition::new_mode(self)
                }
            }
        } else if let Event::Key(KeyEvent {
            code: KeyCode::Char(key),
            modifiers,
        }) = evt
        {
            if !(*modifiers & !KeyModifiers::SHIFT).is_empty() {
                return ModeTransition::not_handled(self);
            }

            if self.hex {
                transition_hex_insertion(self, *key, buffer, options, self.before, self.hex_half)
            } else {
                transition_ascii_insertion(self, *key, buffer, options)
            }
        } else {
            ModeTransition::not_handled(self)
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
