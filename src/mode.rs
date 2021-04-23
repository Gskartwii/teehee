use crossterm::event::Event;
use std::borrow::Cow;

use super::buffer::Buffers;
use super::view::view_options::ViewOptions;

// A mode should OWN all data related to it. Hence we bound it by 'static.
pub trait Mode: 'static {
    // TODO: Maybe this should be just String instead.
    fn name(&self) -> Cow<'static, str>;
    fn transition(
        self: Box<Self>,
        event: &Event,
        buffers: &mut Buffers,
        options: &mut ViewOptions,
    ) -> ModeTransition;

    fn takes_input(&self) -> bool {
        true
    }
    fn has_half_cursor(&self) -> bool {
        false
    }
    fn as_any(&self) -> &dyn std::any::Any;
}

pub enum ModeTransition {
    NotHandled(Box<dyn Mode>),
    Push(Vec<Box<dyn Mode>>),
    Pop,
}

impl ModeTransition {
    pub fn new_mode(mode: impl Mode) -> Self {
        ModeTransition::Push(
            vec![Box::new(mode)],
        )
    }
    pub fn nest_mode(parent: impl Mode, nested: impl Mode) -> Self {
        ModeTransition::Push(
            vec![Box::new(parent), Box::new(nested)],
        )
    }
    pub fn not_handled(mode: impl Mode) -> Self {
        ModeTransition::NotHandled(
            Box::new(mode),
        )
    }
    pub fn pop() -> Self {
        ModeTransition::Pop
    }
}
