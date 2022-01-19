use std::borrow::Cow;

use crossterm::event::Event;

use crate::modes::search::{Pattern, SearchAcceptor};
use crate::modes::{
    mode::{Mode, ModeTransition},
    normal::Normal,
};
use crate::selection::SelRegion;
use crate::Buffers;

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Collapse();

impl SearchAcceptor for Collapse {
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
        if matched_len == 0 {
            // Nothing selected was matched: refuse to split because it would yield
            // an empty selection (invalid)
            return ModeTransition::new_mode(Normal::new());
        }

        let mut remaining_matched_ranges = &matched_ranges[..];
        ModeTransition::new_mode_and_dirty(
            Normal::new(),
            buffer.map_selections(|base_region| {
                let (this, next) = remaining_matched_ranges.split_first().unwrap();
                remaining_matched_ranges = next;

                this.iter()
                    .map(|x| SelRegion::new(x.start, x.end - 1).inherit_direction(&base_region))
                    .collect()
            }),
        )
    }
}

impl Mode for Collapse {
    fn name(&self) -> Cow<'static, str> {
        "COLLAPSE".into()
    }

    fn transition(&self, _: &Event, _: &mut Buffers, _: usize) -> Option<ModeTransition> {
        None
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
