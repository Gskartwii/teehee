use super::buffer::*;
use super::keymap::*;
use super::mode::*;
use super::modes::normal::Normal;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use jetscii::ByteSubstring;
use lazy_static::lazy_static;
use regex::bytes::RegexBuilder;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PatternPiece {
    Literal(u8),
    Wildcard,
}

#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct Pattern {
    pub pieces: Vec<PatternPiece>,
}

impl Pattern {
    fn insert_literal(&mut self, position: usize, literal: u8) -> usize {
        self.pieces.insert(position, PatternPiece::Literal(literal));
        position + 1
    }
    fn insert_half_literal(&mut self, position: usize, literal: u8) -> usize {
        self.pieces[position] = PatternPiece::Literal(literal);
        position + 1
    }
    fn insert_wildcard(&mut self, position: usize) -> usize {
        self.pieces.insert(position, PatternPiece::Wildcard);
        position + 1
    }
    fn remove(&mut self, position: usize) -> bool {
        if position < self.pieces.len() {
            self.pieces.remove(position);
            true
        } else {
            false
        }
    }

    fn as_basic_slice(&self) -> Option<Vec<u8>> {
        self.pieces
            .iter()
            .copied()
            .map(|x| {
                if let PatternPiece::Literal(c) = x {
                    Some(c)
                } else {
                    None
                }
            })
            .collect::<Option<Vec<_>>>()
    }

    pub fn map_selections_to_matches(&self, buffer: &Buffer) -> Vec<Vec<Range<usize>>> {
        if let Some(basic_subslice) = self.as_basic_slice() {
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
            let expr = self
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
        }
    }
}

pub trait SearchAcceptor: Mode {
    fn apply_search(
        &self,
        pattern: Pattern,
        buffer: &mut Buffer,
        bytes_per_line: usize,
    ) -> ModeTransition;
}

pub struct Search {
    pub pattern: Pattern,
    pub cursor: usize,
    pub hex: bool,
    pub hex_half: Option<u8>,
    pub next: RefCell<Option<Box<dyn SearchAcceptor>>>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Action {
    InsertNull,
    InsertWilcard,
    RemoveLast,
    RemoveThis,
    CursorLeft,
    CursorRight,
    SwitchInputMode,
    Finish,
    Cancel,
}

fn default_maps() -> KeyMap<Action> {
    KeyMap {
        maps: keys!(
            (key KeyCode::Backspace => Action::RemoveLast),
            (key KeyCode::Delete => Action::RemoveThis),
            (key KeyCode::Enter => Action::Finish),
            (key KeyCode::Esc => Action::Cancel),
            (key KeyCode::Left => Action::CursorLeft),
            (key KeyCode::Right => Action::CursorRight),
            (ctrl 'o' => Action::SwitchInputMode ),
            (ctrl 'n' => Action::InsertNull),
            (ctrl 'w' => Action::InsertWilcard)
        ),
    }
}

lazy_static! {
    static ref DEFAULT_MAPS: KeyMap<Action> = default_maps();
}

impl Search {
    pub fn new(next: impl SearchAcceptor, hex: bool) -> Search {
        Search {
            next: RefCell::new(Some(Box::new(next))),
            hex,
            hex_half: None,
            cursor: 0,
            pattern: Pattern::default(),
        }
    }
}

impl Mode for Search {
    fn name(&self) -> Cow<'static, str> {
        self.next
            .borrow()
            .as_ref()
            .unwrap()
            .name()
            .to_owned()
            .into()
    }

    fn transition(
        &self,
        evt: &Event,
        buffer: &mut Buffer,
        bytes_per_line: usize,
    ) -> Option<ModeTransition> {
        if let Some(action) = DEFAULT_MAPS.event_to_action(evt) {
            let mut cursor = self.cursor;
            let mut pattern = self.pattern.to_owned();
            let mut hex = self.hex;

            if self.hex_half.is_some() {
                // hex insertion in progress: leave it as-is and skip to the next char
                cursor += 1;
            }

            match action {
                Action::InsertNull => cursor = pattern.insert_literal(cursor, 0),
                Action::InsertWilcard => cursor = pattern.insert_wildcard(cursor),
                Action::RemoveLast if cursor != 0 => {
                    pattern.remove(cursor - 1);
                    cursor -= 1;
                }
                Action::RemoveLast => return Some(ModeTransition::None),
                Action::RemoveThis => {
                    pattern.remove(cursor);
                } // Don't move the cursor
                Action::CursorLeft if cursor != 0 => {
                    cursor -= 1;
                }
                Action::CursorLeft => {}
                Action::CursorRight if cursor < pattern.pieces.len() => {
                    cursor += 1;
                }
                Action::CursorRight => {}
                Action::SwitchInputMode => {
                    hex = !hex;
                }
                Action::Cancel => return Some(ModeTransition::new_mode(Normal::new())),
                Action::Finish => {
                    return Some(self.next.borrow().as_ref().unwrap().apply_search(
                        pattern,
                        buffer,
                        bytes_per_line,
                    ))
                }
            }
            Some(ModeTransition::new_mode(Search {
                pattern,
                cursor,
                hex,
                hex_half: None, // after any action that doesn't insert a hex half, the hex half should be reset
                next: RefCell::new(self.next.replace(None)),
            })) // The old state won't be valid after this
        } else if let Event::Key(KeyEvent {
            code: KeyCode::Char(ch),
            modifiers,
        }) = evt
        {
            if !modifiers.is_empty() {
                return None;
            }
            let mut pattern = self.pattern.to_owned();
            let mut cursor = self.cursor;
            let mut hex_half = self.hex_half;
            if !self.hex {
                cursor = pattern.insert_literal(cursor, *ch as u8);
            } else {
                if !ch.is_ascii_hexdigit() {
                    return None;
                }
                let hex_digit = ch.to_digit(16).unwrap() as u8;
                if let Some(half) = hex_half {
                    cursor = pattern.insert_half_literal(cursor, half | hex_digit);
                    hex_half = None;
                } else {
                    pattern.insert_literal(cursor, hex_digit << 4); // Ignore cursor update
                    hex_half = Some(hex_digit << 4);
                }
            }
            Some(ModeTransition::new_mode(Search {
                pattern,
                cursor,
                hex_half,
                hex: self.hex,
                next: RefCell::new(self.next.replace(None)),
            })) // The old state won't be valid after this
        } else {
            None
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
