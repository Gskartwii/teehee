use std::cmp;
use std::collections::{BTreeSet, HashMap};

use super::buffer::*;
use super::keymap::*;
use super::selection::*;
use super::state::*;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use lazy_static::lazy_static;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Action {
    Width(usize),
    Null,
}

fn default_maps() -> KeyMap<Action> {
    KeyMap {
        maps: keys!(
            ('b' => Action::Width(1)),
            ('w' => Action::Width(2)),
            ('d' => Action::Width(4)),
            ('q' => Action::Width(8)),
            ('o' => Action::Width(16)),
            ('n' => Action::Null)
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
            Action::Null => {
                let null_positions = buffer
                    .selection
                    .iter()
                    .map(|x| {
                        buffer
                            .data
                            .slice_to_cow(x.min()..=x.max())
                            .iter()
                            .enumerate()
                            .filter_map(
                                move |(i, &byte)| if byte == 0 { Some(x.min() + i) } else { None },
                            )
                            .collect::<Vec<_>>()
                        // we must make a temporary vec here, to not keep the Cow<[u8]>
                        // borrowed (which it would be otherwise, as iterators are lazy)
                    })
                    .flatten()
                    .collect::<BTreeSet<_>>();

                if null_positions.len() == buffer.selection.len_bytes() {
                    // Everything selected is a null byte: refuse to split because it would yield
                    // an empty selection (invalid)
                    return Some(StateTransition::NewState(State::Normal));
                }

                StateTransition::StateAndDirtyBytes(
                    State::Normal,
                    buffer.map_selections(|mut base_region| {
                        let mut out = vec![];
                        let mut remaining = true;

                        for &pos in null_positions.range(base_region.min()..=base_region.max()) {
                            let (left_region, right_region) = base_region.split_at(pos);
                            if let Some(left) = left_region {
                                out.push(left);
                            }
                            base_region = if let Some(right) = right_region {
                                right
                            } else {
                                remaining = false;
                                break;
                            }
                        }

                        if remaining {
                            out.push(base_region);
                        }

                        out
                    }),
                )
            }
        })
    } else if let Event::Key(_) = evt {
        Some(StateTransition::NewState(State::Normal))
    } else {
        None
    }
}
