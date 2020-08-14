use std::borrow::Cow;
use std::cmp;
use std::collections::HashMap;

use super::buffer::*;
use super::keymap::*;
use super::mode::*;
use super::modes::normal::Normal;
use super::modes::search::{Pattern, PatternPiece, Search, SearchAcceptor};
use super::selection::*;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use lazy_static::lazy_static;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Split {
    pub count: Option<usize>,
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
    fn apply_search(&self, pattern: Pattern, buffer: &mut Buffer, _: usize) -> ModeTransition {
        if pattern.pieces.is_empty() {
            return ModeTransition::new_mode(Normal());
        }
        let matched_ranges = pattern.map_selections_to_matches(&buffer);
        let matched_len: usize = matched_ranges
            .iter()
            .flatten()
            .map(|r| r.end - r.start)
            .sum();
        if matched_len == buffer.selection.len_bytes() {
            // Everything selected was matched: refuse to split because it would yield
            // an empty selection (invalid)
            return ModeTransition::new_mode(Normal());
        }

        let mut remaining_matched_ranges = &matched_ranges[..];

        ModeTransition::new_mode_and_dirty(
            Normal(),
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
        match self.count {
            None => "SPLIT".into(),
            Some(cnt) => format!("SPLIT ({})", cnt).into(),
        }
    }

    fn transition(
        &self,
        evt: &Event,
        buffer: &mut Buffer,
        bytes_per_line: usize,
    ) -> Option<ModeTransition> {
        if let Some(action) = DEFAULT_MAPS.event_to_action(evt) {
            let count = self.count.unwrap_or(1);
            Some(match action {
                Action::Width(width) => ModeTransition::new_mode_and_dirty(
                    Normal(),
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
                    buffer,
                    bytes_per_line,
                ),
                Action::Search { hex } => ModeTransition::new_mode(Search::new(*self, hex)),
            })
        } else if let Event::Key(KeyEvent {
            code: KeyCode::Char(ch),
            ..
        }) = evt
        {
            if !ch.is_ascii_digit() {
                return Some(ModeTransition::new_mode(Normal()));
            }
            let added = ch.to_digit(10).unwrap() as usize;

            if added == 0 && self.count == None {
                // Doesn't make sense to have 0 for the count
                return Some(ModeTransition::new_mode(Split { count: None }));
            }

            let new_count = self.count.map_or(added, |old| old * 10 + added);
            Some(ModeTransition::new_mode(Split {
                count: Some(new_count),
            }))
        } else if let Event::Key(_) = evt {
            Some(ModeTransition::new_mode(Normal()))
        } else {
            None
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
