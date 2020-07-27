mod buffer;
mod byte_rope;
pub mod hex_view;
#[macro_use]
mod keymap;
mod operations;
mod selection;
mod state;

mod insert_mode;
mod jumpto_mode;
mod normal_mode;
mod replace_mode;
mod split_mode;
mod modes {
    pub(crate) use super::insert_mode as insert;
    pub(crate) use super::jumpto_mode as jumpto;
    pub(crate) use super::normal_mode as normal;
    pub(crate) use super::replace_mode as replace;
    pub(crate) use super::split_mode as split;
}
