use super::buffer::Buffers;
use super::mode::{Mode, ModeTransition};
use super::modes::normal::Normal;
use super::modes::search::{Pattern, SearchAcceptor};
use super::selection::SelRegion;
use super::view::view_options::ViewOptions;
use std::borrow::Cow;

use crossterm::event::Event;

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Collapse();

impl SearchAcceptor for Collapse {
    fn apply_search(
        &self,
        pattern: Pattern,
        buffers: &mut Buffers,
        options: &mut ViewOptions,
    ) -> ModeTransition {
        let buffer = buffers.current_mut();
        if pattern.pieces.is_empty() {
            return ModeTransition::new_mode(Normal::new());
        }
        let matched_ranges = pattern.map_selections_to_matches(&buffer);
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
        options.make_dirty(buffer.map_selections(|base_region| {
            let (this, next) = remaining_matched_ranges.split_first().unwrap();
            remaining_matched_ranges = next;

            this.into_iter()
                .map(|x| SelRegion::new(x.start, x.end - 1).inherit_direction(&base_region))
                .collect()
        }));
        ModeTransition::new_mode(Normal::new())
    }
}

impl Mode for Collapse {
    fn name(&self) -> Cow<'static, str> {
        "COLLAPSE".into()
    }
    fn transition(self, _: &Event, _: &mut Buffers, options: &mut ViewOptions) -> ModeTransition {
        ModeTransition::not_handled(self)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
