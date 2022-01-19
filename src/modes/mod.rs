pub mod quitting {
    use std::borrow::Cow;

    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    pub struct Quitting();

    impl crate::modes::mode::Mode for Quitting {
        fn name(&self) -> Cow<'static, str> {
            "QUITTING".into()
        }
        fn takes_input(&self) -> bool {
            false
        }
        fn transition(
            &self,
            _: &crossterm::event::Event,
            _: &mut crate::buffer::Buffers,
            _: usize,
        ) -> Option<crate::modes::mode::ModeTransition> {
            unreachable!();
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }
}

pub mod collapse;
pub mod command;
pub mod insert;
pub mod jumpto;
pub mod mode;
pub mod normal;
pub mod replace;
pub mod search;
pub mod split;
