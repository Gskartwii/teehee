use crossterm::event::Event;
use std::borrow::Cow;
use xi_rope::Interval;

use super::buffer::Buffer;

// A mode should OWN all data related to it. Hence we bound it by 'static.
pub trait Mode: 'static {
    fn name(&self) -> Cow<str>;
    fn transition(
        &self,
        event: &Event,
        buffer: &mut Buffer,
        bytes_per_line: usize,
    ) -> Option<ModeTransition>;

    fn takes_input(&self) -> bool {
        true
    }
    fn has_half_cursor(&self) -> bool {
        false
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum DirtyBytes {
    ChangeInPlace(Vec<Interval>),
    ChangeLength,
}

pub enum ModeTransition {
    None,
    NewMode(Box<dyn Mode>),
    DirtyBytes(DirtyBytes),
    ModeAndDirtyBytes(Box<dyn Mode>, DirtyBytes),
}

impl ModeTransition {
    pub fn new_mode(mode: impl Mode) -> ModeTransition {
        ModeTransition::NewMode(Box::new(mode))
    }
    pub fn new_mode_and_dirty(mode: impl Mode, dirty: DirtyBytes) -> ModeTransition {
        ModeTransition::ModeAndDirtyBytes(Box::new(mode), dirty)
    }
}
