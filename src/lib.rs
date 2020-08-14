mod buffer;
mod byte_rope;
pub mod hex_view;
#[macro_use]
mod keymap;
mod mode;
mod operations;
mod selection;

mod collapse_mode;
mod insert_mode;
mod jumpto_mode;
mod normal_mode;
mod replace_mode;
mod search_mode;
mod split_mode;
mod modes {
    pub mod quitting {
        use std::borrow::Cow;

        #[derive(Debug, PartialEq, Eq, Clone, Copy)]
        pub struct Quitting();
        impl crate::mode::Mode for Quitting {
            fn name(&self) -> Cow<'static, str> {
                "QUITTING".into()
            }
            fn takes_input(&self) -> bool {
                false
            }
            fn transition(
                &self,
                _: &crossterm::event::Event,
                _: &mut crate::buffer::Buffer,
                _: usize,
            ) -> Option<crate::mode::ModeTransition> {
                unreachable!();
            }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
        }
    }

    pub(crate) use super::collapse_mode as collapse;
    pub(crate) use super::insert_mode as insert;
    pub(crate) use super::jumpto_mode as jumpto;
    pub(crate) use super::normal_mode as normal;
    pub(crate) use super::replace_mode as replace;
    pub(crate) use super::search_mode as search;
    pub(crate) use super::split_mode as split;
}
