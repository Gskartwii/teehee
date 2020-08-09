use std::cmp;
use std::collections::HashMap;
use std::ops::RangeInclusive;

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

pub fn transition(
    evt: &Event,
    buffer: &mut Buffer,
    count: Option<usize>,
) -> Option<StateTransition> {
    if let Some(action) = DEFAULT_MAPS.event_to_action(evt) {
        let count = count.unwrap_or(1);
        Some(match action {
            Action::Width(width) => StateTransition::StateAndDirtyBytes(
                State::Normal,
                buffer.map_selections(|region| {
                    (region.min()..=region.max())
                        .step_by(width * count)
                        .map(|pos| {
                            SelRegion::new(pos, cmp::min(region.max(), pos + width * count - 1))
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
                        let base = x.min();

                        buffer
                            .data
                            .slice_to_cow(x.min()..=x.max())
                            .iter()
                            .enumerate()
                            .fold(
                                vec![],
                                move |mut acc: Vec<RangeInclusive<usize>>, (i, &byte)| {
                                    if byte == 0 {
                                        let len = acc.len();
                                        if len > 0 {
                                            if *acc[len - 1].end() + 1 == i + base {
                                                acc[len - 1] = *acc[len - 1].start()..=(i + base);
                                                return acc;
                                            }
                                        }
                                        acc.push((base + i)..=(base + i));
                                    }
                                    acc
                                },
                            )
                            .into_iter()
                            .filter(|interval| interval.end() - interval.start() + 1 >= count)
                            .collect::<Vec<_>>()
                        // we must make a temporary vec here, to not keep the Cow<[u8]>
                        // borrowed (which it would be otherwise, as iterators are lazy)
                    })
                    .collect::<Vec<_>>();

                let null_bytes_len: usize = null_positions
                    .iter()
                    .flatten()
                    .map(|r| r.end() - r.start())
                    .sum();
                if null_bytes_len == buffer.selection.len_bytes() {
                    // Everything selected is a null byte: refuse to split because it would yield
                    // an empty selection (invalid)
                    return Some(StateTransition::NewState(State::Normal));
                }

                let mut remaining_null_ranges = &null_positions[..];

                StateTransition::StateAndDirtyBytes(
                    State::Normal,
                    buffer.map_selections(|mut base_region| {
                        let mut out = vec![];
                        let mut remaining = true;

                        for range in &remaining_null_ranges[0] {
                            let (left_region, right_region) =
                                base_region.split_at_region(*range.start(), *range.end());
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
                        remaining_null_ranges = &remaining_null_ranges[1..];

                        if remaining {
                            out.push(base_region);
                        }

                        out
                    }),
                )
            }
        })
    } else if let Event::Key(KeyEvent {
        code: KeyCode::Char(ch),
        ..
    }) = evt
    {
        if !ch.is_ascii_digit() {
            return Some(StateTransition::NewState(State::Normal));
        }
        let added = ch.to_digit(10).unwrap() as usize;

        if added == 0 && count == None {
            // Doesn't make sense to have 0 for the count
            return Some(StateTransition::NewState(State::Split { count: None }));
        }

        let new_count = count.map_or(added, |old| old * 10 + added);
        Some(StateTransition::NewState(State::Split {
            count: Some(new_count),
        }))
    } else if let Event::Key(_) = evt {
        Some(StateTransition::NewState(State::Normal))
    } else {
        None
    }
}
