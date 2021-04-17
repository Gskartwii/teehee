use crossterm::event::Event;
use std::borrow::Cow;

use super::buffer::Buffers;
use super::view::view_options::ViewOptions;

// A mode should OWN all data related to it. Hence we bound it by 'static.
pub trait Mode: 'static {
    // TODO: Maybe this should be just String instead.
    fn name(&self) -> Cow<'static, str>;
    fn transition(
        self,
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

pub struct ModeTransition {
    next_mode: Box<dyn Mode>,
    handled: bool,
}

impl ModeTransition {
    pub fn new_mode(mode: impl Mode) -> Self {
        ModeTransition{
            next_mode: Box::new(mode),
            handled: true,
        }
    }
    pub fn not_handled(mode: impl Mode) -> Self {
        ModeTransition{
            next_mode: Box::new(mode),
            handled: false,
        }
    }
}
