use std::cmp;
use std::collections::HashMap;

use super::buffer::*;
use super::keymap::*;
use super::selection::*;
use super::state::*;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use lazy_static::lazy_static;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Action {
    Width(usize),
}

fn default_maps() -> KeyMap<Action> {
    KeyMap {
        maps: keys!(
            ('b' => Action::Width(1)),
            ('w' => Action::Width(2)),
            ('d' => Action::Width(4)),
            ('q' => Action::Width(8)),
            ('o' => Action::Width(16))
        ),
    }
}

lazy_static! {
    static ref DEFAULT_MAPS: KeyMap<Action> = default_maps();
}

pub fn transition(evt: &Event, buffer: &mut Buffer) -> Option<StateTransition> {
    if let Some(action) = DEFAULT_MAPS.event_to_action(evt) {
        Some(match action {
            Action::Width(width) => StateTransition::StateAndDirtyBytes(
                State::Normal,
                buffer.map_selections(|region| {
                    (region.min()..=region.max())
                        .step_by(width)
                        .map(|pos| {
                            SelRegion::new(pos, cmp::min(region.max(), pos + width - 1))
                                .with_direction(region.backward())
                        })
                        .collect()
                }),
            ),
        })
    } else if let Event::Key(_) = evt {
        Some(StateTransition::NewState(State::Normal))
    } else {
        None
    }
}
