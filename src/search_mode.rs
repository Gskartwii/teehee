use super::buffer::*;
use super::keymap::*;
use super::mode::*;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use lazy_static::lazy_static;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum PatternPiece {
    Literal(u8),
    Wildcard,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Pattern {
    pieces: Vec<PatternPiece>,
}

impl Pattern {
    fn insert_literal(&mut self, position: usize, literal: u8) -> usize {
        self.pieces.insert(position, PatternPiece::Literal(literal));
        position + 1
    }
    fn insert_wildcard(&mut self, position: usize) -> usize {
        self.pieces.insert(position, PatternPiece::Wildcard);
        position + 1
    }
    fn remove(&mut self, position: usize) -> usize {
        if position < self.pieces.len() {
            self.pieces.remove(position);
            position - 1
        } else {
            position
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
    pattern: Pattern,
    cursor: usize,
    next: RefCell<Option<Box<dyn SearchAcceptor>>>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Action {
    InsertNull,
    InsertWilcard,
    RemoveLast,
    RemoveThis,
    Finish,
}

fn default_maps() -> KeyMap<Action> {
    KeyMap {
        maps: keys!(
            (key KeyCode::Backspace => Action::RemoveLast),
            (key KeyCode::Delete => Action::RemoveThis),
            (key KeyCode::Enter => Action::Finish),
            (ctrl 'n' => Action::InsertNull),
            (ctrl 'w' => Action::InsertWilcard)
        ),
    }
}

lazy_static! {
    static ref DEFAULT_MAPS: KeyMap<Action> = default_maps();
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
            match action {
                Action::InsertNull => cursor = pattern.insert_literal(cursor, 0),
                Action::InsertWilcard => cursor = pattern.insert_wildcard(cursor),
                Action::RemoveLast if cursor != 0 => {
                    pattern.remove(cursor - 1);
                    cursor -= 1;
                }
                Action::RemoveLast => return Some(ModeTransition::None),
                Action::RemoveThis => cursor = pattern.remove(cursor),
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
            None
        } else {
            None
        }
    }
}
