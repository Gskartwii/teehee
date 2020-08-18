use std::borrow::Cow;
use std::collections::HashMap;

use super::buffer::*;
use super::keymap::*;
use super::mode::*;
use super::modes::normal::Normal;
use super::selection::*;

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
        &self,
        evt: &Event,
        buffer: &mut Buffer,
        bytes_per_line: usize,
    ) -> Option<ModeTransition> {
        if let Some(direction) = DEFAULT_MAPS.event_to_action(evt) {
            let max_bytes = buffer.data.len();
            Some(ModeTransition::new_mode_and_dirty(
                Normal::new(),
                if self.extend {
                    buffer.map_selections(|region| {
                        vec![region.extend_to_boundary(direction, bytes_per_line, max_bytes)]
                    })
                } else {
                    buffer.map_selections(|region| {
                        vec![region.jump_to_boundary(direction, bytes_per_line, max_bytes)]
                    })
                },
            ))
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
