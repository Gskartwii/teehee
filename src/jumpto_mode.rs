use std::collections::HashMap;

use super::buffer::*;
use super::keymap::*;
use super::selection::*;
use super::state::*;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use lazy_static::lazy_static;

fn default_maps() -> KeyMap<Direction> {
    KeyMap {
        maps: keys!(
            ('h' => Direction::Left),
            ('j' => Direction::Down),
            ('k' => Direction::Up),
            ('l' => Direction::Right)
        ),
    }
}

lazy_static! {
    static ref DEFAULT_MAPS: KeyMap<Direction> = default_maps();
}

pub fn transition(
    evt: &Event,
    buffer: &mut Buffer,
    extend: bool,
    bytes_per_line: usize,
) -> Option<StateTransition> {
    if let Some(direction) = DEFAULT_MAPS.event_to_action(evt) {
        let max_bytes = buffer.data.len();
        Some(StateTransition::StateAndDirtyBytes(
            State::Normal,
            if extend {
                buffer.map_selections(|region| {
                    vec![region.extend_to_boundary(direction, bytes_per_line, max_bytes)]
                })
            } else {
                buffer.map_selections(|region| {
                    vec![region.jump_to_boundary(direction, bytes_per_line, max_bytes)]
                })
            },
        ))
    } else if let Event::Key(_) = evt {
        Some(StateTransition::NewState(State::Normal))
    } else {
        None
    }
}
