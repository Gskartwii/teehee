use std::borrow::Cow;
use std::collections::HashMap;

use super::buffer::*;
use super::keymap::*;
use super::mode::*;
use super::modes::normal::Normal;
use super::operations as ops;

use crate::selection::Direction;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use lazy_static::lazy_static;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Replace {
    pub hex: bool,
    pub hex_half: Option<u8>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Action {
    Null,
    Move(Direction),
}

fn default_maps() -> KeyMap<Action> {
    KeyMap {
        maps: keys!(
            (ctrl 'n' => Action::Null),
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

impl Mode for Replace {
    fn name(&self) -> Cow<'static, str> {
        match (self.hex, self.hex_half) {
            (true, None) => "REPLACE (hex)".into(),
            (false, _) => "REPLACE (ascii)".into(),
            (true, Some(ch)) => format!("REPLACE (hex: {:x}...)", ch >> 4).into(),
        }
    }

    fn transition(
        &self,
        evt: &Event,
        buffers: &mut Buffers,
        bytes_per_line: usize,
    ) -> Option<ModeTransition> {
        let buffer = buffers.current_mut();

        if let Some(action) = DEFAULT_MAPS.event_to_action(evt) {
            return match action {
                Action::Null => {
                    let delta = ops::replace(&buffer.data, &buffer.selection, 0);
                    Some(ModeTransition::new_mode_and_dirty(
                        Normal::new(),
                        buffer.apply_delta(delta),
                    ))
                }
                Action::Move(direction) => {
                    // we should not move if user already write a half of the hex byte
                    if self.hex_half.is_none() {
                        let max_bytes = buffer.data.len();
                        Some(ModeTransition::new_mode_and_dirty(
                            Replace {
                                hex: self.hex,
                                hex_half: self.hex_half,
                            },
                            buffer.map_selections(|region| {
                                vec![region.simple_move(direction, bytes_per_line, max_bytes, 1)]
                            }),
                        ))
                    } else {
                        Some(ModeTransition::new_mode(Replace {
                            hex: self.hex,
                            hex_half: self.hex_half,
                        }))
                    }
                }
            };
        }

        if let Event::Key(KeyEvent {
            code: KeyCode::Char(ch),
            modifiers,
        }) = evt
        {
            if !(*modifiers & !KeyModifiers::SHIFT).is_empty() {
                return Some(ModeTransition::new_mode(Normal::new()));
            }

            if !self.hex {
                let delta = ops::replace(&buffer.data, &buffer.selection, *ch as u8); // lossy!
                Some(ModeTransition::new_mode_and_dirty(
                    Replace {
                        hex: self.hex,
                        hex_half: self.hex_half,
                    },
                    buffer.apply_delta(delta),
                ))
            } else if self.hex_half.is_none() {
                if !ch.is_ascii_hexdigit() {
                    return Some(ModeTransition::new_mode(Normal::new()));
                }

                let replacing_ch = (ch.to_digit(16).unwrap() as u8) << 4;
                Some(ModeTransition::new_mode(Replace {
                    hex: self.hex,
                    hex_half: Some(replacing_ch),
                }))
            } else {
                if !ch.is_ascii_hexdigit() {
                    return Some(ModeTransition::new_mode(Normal::new()));
                }

                let replacing_ch = (ch.to_digit(16).unwrap() as u8) | self.hex_half.unwrap();
                let delta = ops::replace(&buffer.data, &buffer.selection, replacing_ch); // lossy!
                Some(ModeTransition::new_mode_and_dirty(
                    Replace {
                        hex: self.hex,
                        hex_half: None,
                    },
                    buffer.apply_delta(delta),
                ))
            }
        } else if let Event::Key(_) = evt {
            Some(ModeTransition::new_mode(Normal::new()))
        } else {
            None
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
