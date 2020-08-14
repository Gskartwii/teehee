use std::borrow::Cow;
use std::cell::RefCell;
use std::cmp;
use std::collections::HashMap;
use std::ops::RangeInclusive;

use super::buffer::*;
use super::keymap::*;
use super::mode::*;
use super::modes::normal::Normal;
use super::modes::search::{Pattern, PatternPiece, Search, SearchAcceptor};
use super::selection::*;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use jetscii::ByteSubstring;
use lazy_static::lazy_static;
use regex::bytes::RegexBuilder;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Split {
    pub count: Option<usize>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Action {
    Width(usize),
    Null,
    Search,
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
            ('/' => Action::Search)
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

        let matched_ranges = if let Some(basic_subslice) = pattern.as_basic_slice() {
            buffer
                .selection
                .iter()
                .map(|x| {
                    let mut base = x.min();
                    let mut matched_ranges = vec![];
                    let byte_substring = ByteSubstring::new(&basic_subslice);

                    while let Some(start) =
                        byte_substring.find(&buffer.data.slice_to_cow(base..=x.max()))
                    {
                        let match_abs_start = base + start;
                        matched_ranges
                            .push(match_abs_start..match_abs_start + basic_subslice.len());
                        base = match_abs_start + basic_subslice.len();
                    }
                    matched_ranges
                })
                .collect::<Vec<_>>()
        } else {
            let expr = pattern
                .pieces
                .iter()
                .map(|x| match x {
                    PatternPiece::Wildcard => Cow::from("."),
                    PatternPiece::Literal(c) => Cow::from(format!("\\x{:02x}", c)),
                })
                .collect::<String>();
            let mut builder = RegexBuilder::new(&expr);
            builder.unicode(false);
            let matcher = builder.build().expect("Failed to create pattern");

            buffer
                .selection
                .iter()
                .map(|x| {
                    matcher
                        .find_iter(&buffer.data.slice_to_cow(x.min()..=x.max()))
                        .map(|r| (x.min() + r.start())..(x.min() + r.end()))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };

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

    fn transition(&self, evt: &Event, buffer: &mut Buffer, _: usize) -> Option<ModeTransition> {
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
                                                    acc[len - 1] =
                                                        *acc[len - 1].start()..=(i + base);
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
                        return Some(ModeTransition::new_mode(Normal()));
                    }

                    let mut remaining_null_ranges = &null_positions[..];

                    ModeTransition::new_mode_and_dirty(
                        Normal(),
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
                Action::Search => ModeTransition::new_mode(Search {
                    hex: true,
                    hex_half: None,
                    cursor: 0,
                    pattern: Pattern::default(),
                    next: RefCell::new(Some(Box::new(*self))),
                }),
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
