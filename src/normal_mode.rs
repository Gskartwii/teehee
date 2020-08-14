use std::borrow::Cow;
use std::collections::HashMap;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use lazy_static::lazy_static;

use super::buffer::*;
use super::keymap::*;
use super::mode::*;
use super::modes;
use super::operations as ops;
use super::selection::Direction;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Normal();

#[derive(Debug, PartialEq, Clone, Copy)]
enum Action {
    Quit,
    Move(Direction),
    Extend(Direction),
    SplitMode,
    JumpToMode,
    ExtendToMode,
    CollapseMode { hex: bool },
    SwapCaret,
    CollapseSelection,
    Delete { register: char },
    Yank { register: char },
    Paste { after: bool, register: char },
    Change { hex: bool, register: char },
    Insert { hex: bool },
    Append { hex: bool },
    RemoveMain,
    RetainMain,
    SelectPrev,
    SelectNext,
    SelectAll,
    ReplaceMode { hex: bool },
}

fn default_maps() -> KeyMap<Action> {
    KeyMap {
        maps: keys!(
            (key KeyCode::Esc => Action::Quit),
            ('h' => Action::Move(Direction::Left)),
            ('j' => Action::Move(Direction::Down)),
            ('k' => Action::Move(Direction::Up)),
            ('l' => Action::Move(Direction::Right)),
            ('H' => Action::Extend(Direction::Left)),
            ('J' => Action::Extend(Direction::Down)),
            ('K' => Action::Extend(Direction::Up)),
            ('L' => Action::Extend(Direction::Right)),
            ('g' => Action::JumpToMode),
            ('G' => Action::ExtendToMode),
            (alt 's' => Action::SplitMode),
            (';' => Action::CollapseSelection),
            (alt ';' => Action::SwapCaret),
            ('%' => Action::SelectAll),
            (' ' => Action::RetainMain),
            (alt ' ' => Action::RemoveMain),
            ('(' => Action::SelectPrev),
            (')' => Action::SelectNext),

            ('p' => Action::Paste{after: true, register: '"'}),
            ('P' => Action::Paste{after: false, register: '"'}),
            ('d' => Action::Delete{register: '"'}),
            ('y' => Action::Yank{register: '"'}),
            ('c' => Action::Change{hex: true, register: '"'}),
            ('C' => Action::Change{hex: false, register: '"'}),

            ('i' => Action::Insert{hex: true}),
            ('I' => Action::Insert{hex: false}),
            ('a' => Action::Append{hex: true}),
            ('A' => Action::Append{hex: false}),
            ('r' => Action::ReplaceMode{hex: true}),
            ('R' => Action::ReplaceMode{hex: false}),

            ('s' => Action::CollapseMode{hex: true}),
            ('S' => Action::CollapseMode{hex: false})
        ),
    }
}

lazy_static! {
    static ref DEFAULT_MAPS: KeyMap<Action> = default_maps();
}

impl Mode for Normal {
    fn name(&self) -> Cow<'static, str> {
        "NORMAL".into()
    }

    fn transition(
        &self,
        event: &Event,
        buffer: &mut Buffer,
        bytes_per_line: usize,
    ) -> Option<ModeTransition> {
        if let Some(action) = DEFAULT_MAPS.event_to_action(event) {
            Some(match action {
                Action::Quit => ModeTransition::new_mode(modes::quitting::Quitting()),
                Action::JumpToMode => {
                    ModeTransition::new_mode(modes::jumpto::JumpTo { extend: false })
                }
                Action::ExtendToMode => {
                    ModeTransition::new_mode(modes::jumpto::JumpTo { extend: true })
                }
                Action::SplitMode => ModeTransition::new_mode(modes::split::Split { count: None }),
                Action::Insert { hex } => ModeTransition::new_mode_and_dirty(
                    modes::insert::Insert {
                        hex,
                        before: true,
                        hex_half: None,
                    },
                    buffer.map_selections(|region| vec![region.to_backward()]),
                ),
                Action::Append { hex } => ModeTransition::new_mode_and_dirty(
                    modes::insert::Insert {
                        hex,
                        before: false,
                        hex_half: None,
                    },
                    {
                        let max_size = buffer.data.len();
                        buffer.map_selections(|region| {
                            vec![region.to_forward().simple_extend(
                                Direction::Right,
                                bytes_per_line,
                                max_size,
                            )]
                        })
                    },
                ),
                Action::ReplaceMode { hex } => ModeTransition::new_mode(modes::replace::Replace {
                    hex,
                    hex_half: None,
                }),
                Action::Move(direction) => {
                    let max_bytes = buffer.data.len();
                    ModeTransition::DirtyBytes(buffer.map_selections(|region| {
                        vec![region.simple_move(direction, bytes_per_line, max_bytes)]
                    }))
                }
                Action::Extend(direction) => {
                    let max_bytes = buffer.data.len();
                    ModeTransition::DirtyBytes(buffer.map_selections(|region| {
                        vec![region.simple_extend(direction, bytes_per_line, max_bytes)]
                    }))
                }
                Action::SwapCaret => ModeTransition::DirtyBytes(
                    buffer.map_selections(|region| vec![region.swap_caret()]),
                ),
                Action::CollapseSelection => ModeTransition::DirtyBytes(
                    buffer.map_selections(|region| vec![region.collapse()]),
                ),
                Action::Delete { register } => {
                    buffer.yank_selections(register);
                    if !buffer.data.is_empty() {
                        let delta = ops::deletion(&buffer.data, &buffer.selection);
                        ModeTransition::DirtyBytes(buffer.apply_delta(&delta))
                    } else {
                        ModeTransition::None
                    }
                }
                Action::Change { hex, register } => {
                    buffer.yank_selections(register);
                    if !buffer.data.is_empty() {
                        let delta = ops::deletion(&buffer.data, &buffer.selection);
                        ModeTransition::new_mode_and_dirty(
                            modes::insert::Insert {
                                hex,
                                before: true,
                                hex_half: None,
                            },
                            buffer.apply_delta(&delta),
                        )
                    } else {
                        ModeTransition::new_mode(modes::insert::Insert {
                            hex,
                            before: true,
                            hex_half: None,
                        })
                    }
                }
                Action::Yank { register } => {
                    buffer.yank_selections(register);
                    ModeTransition::None
                }
                Action::Paste { register, after } => {
                    let delta = ops::paste(
                        &buffer.data,
                        &buffer.selection,
                        &buffer.registers.get(&register).unwrap_or(&vec![vec![]]),
                        after,
                    );
                    ModeTransition::DirtyBytes(buffer.apply_delta(&delta))
                }
                Action::RemoveMain => ModeTransition::DirtyBytes(buffer.remove_main_sel()),
                Action::RetainMain => ModeTransition::DirtyBytes(buffer.retain_main_sel()),
                Action::SelectNext => ModeTransition::DirtyBytes(buffer.select_next()),
                Action::SelectPrev => ModeTransition::DirtyBytes(buffer.select_prev()),
                Action::SelectAll => {
                    buffer.selection.select_all(buffer.data.len());
                    ModeTransition::DirtyBytes(DirtyBytes::ChangeInPlace(vec![(0..buffer
                        .data
                        .len())
                        .into()]))
                }
                Action::CollapseMode { hex } => ModeTransition::new_mode(
                    modes::search::Search::new(modes::collapse::Collapse(), hex),
                ),
            })
        } else {
            None
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
