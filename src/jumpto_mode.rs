use std::borrow::Cow;
use std::collections::HashMap;

use super::buffer::*;
use super::keymap::*;
use super::mode::*;
use super::selection::*;
use super::view::view_options::ViewOptions;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct JumpTo {
    pub extend: bool,
}

use lazy_static::lazy_static;

fn default_maps() -> KeyMap<Direction> {
    KeyMap {
        maps: keys!(
            ('h' => Direction::Left),
            ('j' => Direction::Down),
            ('k' => Direction::Up),
            ('l' => Direction::Right)
        ),
    }
}

lazy_static! {
    static ref DEFAULT_MAPS: KeyMap<Direction> = default_maps();
}

impl Mode for JumpTo {
    fn name(&self) -> Cow<'static, str> {
        if self.extend {
            "EXTEND".into()
        } else {
            "JUMP".into()
        }
    }

    fn transition(
        self: Box<Self>,
        evt: &Event,
        buffers: &mut Buffers,
        options: &mut ViewOptions,
    ) -> ModeTransition {
        let buffer = buffers.current_mut();
        if let Some(direction) = DEFAULT_MAPS.event_to_action(evt) {
            let max_bytes = buffer.data.len();
            if self.extend {
                options.make_dirty(buffer.map_selections(|region| {
                    vec![region.extend_to_boundary(direction, options.bytes_per_line, max_bytes)]
                }));
            } else {
                options.make_dirty(buffer.map_selections(|region| {
                    vec![region.jump_to_boundary(direction, options.bytes_per_line, max_bytes)]
                }));
            }
            ModeTransition::pop()
        } else if let Event::Key(_) = evt {
            ModeTransition::pop()
        } else {
            ModeTransition::not_handled(*self)
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
