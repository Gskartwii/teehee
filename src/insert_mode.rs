use std::collections::HashMap;

use super::buffer::*;
use super::keymap::*;
use super::operations as ops;
use super::state::*;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use lazy_static::lazy_static;

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

fn transition_ascii_insertion(key: char, buffer: &mut Buffer) -> StateTransition {
    let mut inserted_bytes = vec![0u8; key.len_utf8()];
    key.encode_utf8(&mut inserted_bytes);

    // At this point `before` doesn't really matter;
    // the cursors will have been moved in normal mode to their
    // correct places.
    let delta = ops::insert(&buffer.data, &buffer.selection, inserted_bytes);
    StateTransition::DirtyBytes(buffer.apply_delta(&delta))
}

fn transition_hex_insertion(
    key: char,
    buffer: &mut Buffer,
    before: bool,
    hex_half: Option<u8>,
) -> Option<StateTransition> {
    if !key.is_ascii_hexdigit() {
        return None;
    }

    let digit = key.to_digit(16).unwrap() as u8;
    let to_insert = hex_half.map(|x| x | digit).unwrap_or(digit << 4);
    let insert_half = hex_half.is_none();

    if insert_half {
        let delta = ops::insert(&buffer.data, &buffer.selection, vec![to_insert]);
        Some(StateTransition::StateAndDirtyBytes(
            State::Insert {
                before,
                hex: true,
                hex_half: Some(to_insert),
            },
            buffer.apply_delta_offset_carets(&delta, -1, 0),
        ))
    } else {
        let delta = ops::change(&buffer.data, &buffer.selection, vec![to_insert]);
        Some(StateTransition::StateAndDirtyBytes(
            State::Insert {
                before,
                hex: true,
                hex_half: None,
            },
            buffer.apply_delta(&delta),
        ))
    }
}

pub fn transition(
    evt: &Event,
    buffer: &mut Buffer,
    before: bool,
    hex: bool,
    hex_half: Option<u8>,
) -> Option<StateTransition> {
    if let Some(action) = DEFAULT_MAPS.event_to_action(evt) {
        Some(match action {
            Action::Exit => StateTransition::NewState(State::Normal),
            Action::InsertNull => {
                let inserted_bytes = vec![0];
                let delta = ops::insert(&buffer.data, &buffer.selection, inserted_bytes);
                StateTransition::DirtyBytes(buffer.apply_delta(&delta))
            }
            Action::SwitchInputMode => StateTransition::NewState(State::Insert {
                before,
                hex: !hex,
                hex_half: None,
            }),
            Action::RemoveLast => {
                if buffer.data.is_empty() {
                    return Some(StateTransition::None);
                }
                let delta = ops::backspace(&buffer.data, &buffer.selection);
                StateTransition::DirtyBytes(buffer.apply_delta(&delta))
            }
            Action::RemoveThis => {
                if buffer.data.is_empty() {
                    return Some(StateTransition::None);
                }
                let delta = ops::delete_cursor(&buffer.data, &buffer.selection);
                StateTransition::DirtyBytes(buffer.apply_delta(&delta))
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

        if hex {
            transition_hex_insertion(*key, buffer, before, hex_half)
        } else {
            Some(transition_ascii_insertion(*key, buffer))
        }
    } else {
        None
    }
}
