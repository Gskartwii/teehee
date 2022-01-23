use std::borrow::Cow;
use std::cmp;
use std::collections::HashMap;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use lazy_static::lazy_static;

use crate::keymap::KeyMap;
use crate::modes::{
    mode::{Mode, ModeTransition},
    normal::Normal,
    search::{Pattern, PatternPiece, Search, SearchAcceptor},
};
use crate::selection::SelRegion;
use crate::{cmd_count, Buffers};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Split {
    count_state: cmd_count::State,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Action {
    Width(usize),
    Null,
    Search { hex: bool },
}

fn default_maps() -> KeyMap<Action> {
    KeyMap {
        maps: keys!(
            ('b' => Action::Width(1)),
            ('w' => Action::Width(2)),
            ('d' => Action::Width(4)),
            ('q' => Action::Width(8)),
            ('o' => Action::Width(16)),
            ('n' => Action::Null),
            ('/' => Action::Search{hex: false}),
            ('?' => Action::Search{hex: true})
        ),
    }
}

lazy_static! {
    static ref DEFAULT_MAPS: KeyMap<Action> = default_maps();
}

impl SearchAcceptor for Split {
    fn apply_search(&self, pattern: Pattern, buffers: &mut Buffers, _: usize) -> ModeTransition {
        let buffer = buffers.current_mut();
        if pattern.pieces.is_empty() {
            return ModeTransition::new_mode(Normal::new());
        }
        let matched_ranges = pattern.map_selections_to_matches(buffer);
        let matched_len: usize = matched_ranges
            .iter()
            .flatten()
            .map(|r| r.end - r.start)
            .sum();
        if matched_len == buffer.selection.len_bytes() {
            // Everything selected was matched: refuse to split because it would yield
            // an empty selection (invalid)
            return ModeTransition::new_mode(Normal::new());
        }

        let mut remaining_matched_ranges = &matched_ranges[..];

        ModeTransition::new_mode_and_dirty(
            Normal::new(),
            buffer.map_selections(|mut base_region| {
                let mut out = vec![];
                let mut remaining = true;

                for range in &remaining_matched_ranges[0] {
                    let (left_region, right_region) =
                        base_region.split_at_region(range.start, range.end - 1);
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
                remaining_matched_ranges = &remaining_matched_ranges[1..];

                if remaining {
                    out.push(base_region);
                }

                out
            }),
        )
    }
}

impl Mode for Split {
    fn name(&self) -> Cow<'static, str> {
        format!("SPLIT{}", self.count_state).into()
    }

    fn transition(
        &self,
        evt: &Event,
        buffers: &mut Buffers,
        bytes_per_line: usize,
    ) -> Option<ModeTransition> {
        let buffer = buffers.current_mut();
        if let cmd_count::Transition::Update(new_state) = self.count_state.transition(evt) {
            Some(ModeTransition::new_mode(Split {
                count_state: new_state,
            }))
        } else if let Some(action) = DEFAULT_MAPS.event_to_action(evt) {
            let count = self.count_state.to_count();
            Some(match action {
                Action::Width(width) => ModeTransition::new_mode_and_dirty(
                    Normal::new(),
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
                Action::Null => self.apply_search(
                    Pattern {
                        pieces: std::iter::repeat(PatternPiece::Literal(0u8))
                            .take(count)
                            .collect(),
                    },
                    buffers,
                    bytes_per_line,
                ),
                Action::Search { hex } => ModeTransition::new_mode(Search::new(*self, hex)),
            })
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

impl Split {
    pub fn new() -> Split {
        Split {
            count_state: cmd_count::State::None,
        }
    }
}
