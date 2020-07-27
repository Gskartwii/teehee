use std::collections::HashMap;

use super::buffer::*;
use super::keymap::*;
use super::operations as ops;
use super::state::*;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use lazy_static::lazy_static;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Action {
    Null,
}

fn default_maps() -> KeyMap<Action> {
    KeyMap {
        maps: keys!(
            (ctrl 'n' => Action::Null)
        ),
    }
}

lazy_static! {
    static ref DEFAULT_MAPS: KeyMap<Action> = default_maps();
}

pub fn transition(
    evt: &Event,
    buffer: &mut Buffer,
    hex: bool,
    hex_half: Option<u8>,
) -> Option<StateTransition> {
    if let Event::Key(KeyEvent {
        code: KeyCode::Char(ch),
        modifiers,
    }) = evt
    {
        if let Some(action) = DEFAULT_MAPS.event_to_action(evt) {
            return match action {
                Action::Null => {
                    let delta = ops::replace(&buffer.data, &buffer.selection, 0);
                    Some(StateTransition::StateAndDirtyBytes(
                        State::Normal,
                        buffer.apply_delta(&delta),
                    ))
                }
            };
        }

        if !(*modifiers & !KeyModifiers::SHIFT).is_empty() {
            return Some(StateTransition::NewState(State::Normal));
        }

        if !hex {
            let delta = ops::replace(&buffer.data, &buffer.selection, *ch as u8); // lossy!
            Some(StateTransition::StateAndDirtyBytes(
                State::Normal,
                buffer.apply_delta(&delta),
            ))
        } else if hex_half.is_none() {
            if !ch.is_ascii_hexdigit() {
                return Some(StateTransition::NewState(State::Normal));
            }

            let replacing_ch = (ch.to_digit(16).unwrap() as u8) << 4;
            Some(StateTransition::NewState(State::Replace {
                hex,
                hex_half: Some(replacing_ch),
            }))
        } else {
            if !ch.is_ascii_hexdigit() {
                return Some(StateTransition::NewState(State::Normal));
            }

            let replacing_ch = (ch.to_digit(16).unwrap() as u8) | hex_half.unwrap();
            let delta = ops::replace(&buffer.data, &buffer.selection, replacing_ch); // lossy!
            Some(StateTransition::StateAndDirtyBytes(
                State::Normal,
                buffer.apply_delta(&delta),
            ))
        }
    } else if let Event::Key(_) = evt {
        Some(StateTransition::NewState(State::Normal))
    } else {
        None
    }
}
