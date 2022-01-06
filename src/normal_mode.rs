use std::borrow::Cow;
use std::collections::HashMap;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use lazy_static::lazy_static;

use super::buffer::*;
use super::cmd_count;
use super::keymap::*;
use super::mode::*;
use super::modes;
use super::operations as ops;
use super::selection::Direction;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Normal {
    count_state: cmd_count::State,
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Action {
    Move(Direction),
    Extend(Direction),
    SplitMode,
    JumpToMode,
    ExtendToMode,
    CollapseMode { hex: bool },
    CommandMode,
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
    Measure,
    Undo,
    Redo,
}

fn default_maps() -> KeyMap<Action> {
    KeyMap {
        maps: keys!(
            ('h' => Action::Move(Direction::Left)),
            (key KeyCode::Left => Action::Move(Direction::Left)),
            ('j' => Action::Move(Direction::Down)),
            (key KeyCode::Down => Action::Move(Direction::Down)),
            ('k' => Action::Move(Direction::Up)),
            (key KeyCode::Up => Action::Move(Direction::Up)),
            ('l' => Action::Move(Direction::Right)),
            (key KeyCode::Right => Action::Move(Direction::Right)),
            ('H' => Action::Extend(Direction::Left)),
            ('J' => Action::Extend(Direction::Down)),
            ('K' => Action::Extend(Direction::Up)),
            ('L' => Action::Extend(Direction::Right)),
            ('g' => Action::JumpToMode),
            ('G' => Action::ExtendToMode),
            (alt 's' => Action::SplitMode),
            (':' => Action::CommandMode),
            (';' => Action::CollapseSelection),
            (alt ';' => Action::SwapCaret),
            ('%' => Action::SelectAll),
            (' ' => Action::RetainMain),
            (alt ' ' => Action::RemoveMain),
            ('(' => Action::SelectPrev),
            (')' => Action::SelectNext),
            ('M' => Action::Measure),
            ('u' => Action::Undo),
            ('U' => Action::Redo),

            ('p' => Action::Paste{after: true, register: '"'}),
            ('P' => Action::Paste{after: false, register: '"'}),
            ('d' => Action::Delete{register: '"'}),
            ('y' => Action::Yank{register: '"'}),
            ('c' => Action::Change{hex: false, register: '"'}),
            ('C' => Action::Change{hex: true, register: '"'}),

            ('i' => Action::Insert{hex: false}),
            ('I' => Action::Insert{hex: true}),
            ('a' => Action::Append{hex: false}),
            ('A' => Action::Append{hex: true}),
            ('r' => Action::ReplaceMode{hex: false}),
            ('R' => Action::ReplaceMode{hex: true}),

            ('s' => Action::CollapseMode{hex: false}),
            ('S' => Action::CollapseMode{hex: true})
        ),
    }
}

lazy_static! {
    static ref DEFAULT_MAPS: KeyMap<Action> = default_maps();
}

impl Mode for Normal {
    fn name(&self) -> Cow<'static, str> {
        format!("NORMAL{}", self.count_state).into()
    }

    fn transition(
        &self,
        event: &Event,
        buffers: &mut Buffers,
        bytes_per_line: usize,
    ) -> Option<ModeTransition> {
        let buffer = buffers.current_mut();
        if let cmd_count::Transition::Update(new_state) = self.count_state.transition(event) {
            Some(ModeTransition::new_mode(Normal {
                count_state: new_state,
            }))
        } else if let Some(action) = DEFAULT_MAPS.event_to_action(event) {
            Some(match action {
                Action::JumpToMode => match self.count_state {
                    cmd_count::State::None => {
                        ModeTransition::new_mode(modes::jumpto::JumpTo { extend: false })
                    }
                    cmd_count::State::Some { count: offset, .. } => {
                        ModeTransition::new_mode_and_dirty(
                            Normal::new(),
                            buffer.map_selections(|region| vec![region.jump_to(offset)]),
                        )
                    }
                },
                Action::ExtendToMode => match self.count_state {
                    cmd_count::State::None => {
                        ModeTransition::new_mode(modes::jumpto::JumpTo { extend: true })
                    }
                    cmd_count::State::Some { count: offset, .. } => {
                        ModeTransition::new_mode_and_dirty(
                            Normal::new(),
                            buffer.map_selections(|region| vec![region.extend_to(offset)]),
                        )
                    }
                },
                Action::SplitMode => ModeTransition::new_mode(modes::split::Split::new()),
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
                                1,
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
                    ModeTransition::new_mode_and_dirty(
                        Normal::new(),
                        buffer.map_selections(|region| {
                            vec![region.simple_move(
                                direction,
                                bytes_per_line,
                                max_bytes,
                                self.count_state.to_count(),
                            )]
                        }),
                    )
                }
                Action::Extend(direction) => {
                    let max_bytes = buffer.data.len();
                    ModeTransition::new_mode_and_dirty(
                        Normal::new(),
                        buffer.map_selections(|region| {
                            vec![region.simple_extend(
                                direction,
                                bytes_per_line,
                                max_bytes,
                                self.count_state.to_count(),
                            )]
                        }),
                    )
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
                        ModeTransition::DirtyBytes(buffer.apply_delta(delta))
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
                            buffer.apply_delta(delta),
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
                        buffer.registers.get(&register).unwrap_or(&vec![vec![]]),
                        after,
                        self.count_state.to_count(),
                    );
                    ModeTransition::DirtyBytes(buffer.apply_delta(delta))
                }
                // selection indexing in the UI starts at 1
                // hence we check for count > 0 and offset by -1
                Action::RemoveMain => match self.count_state {
                    cmd_count::State::Some { count, .. } if count > 0 => {
                        ModeTransition::new_mode_and_dirty(
                            Normal::new(),
                            buffer.remove_selection(count - 1),
                        )
                    }
                    _ => ModeTransition::DirtyBytes(
                        buffer.remove_selection(buffer.selection.main_selection),
                    ),
                },
                Action::RetainMain => match self.count_state {
                    cmd_count::State::Some { count, .. } if count > 0 => {
                        ModeTransition::new_mode_and_dirty(
                            Normal::new(),
                            buffer.retain_selection(count - 1),
                        )
                    }
                    _ => ModeTransition::DirtyBytes(
                        buffer.retain_selection(buffer.selection.main_selection),
                    ),
                },

                // new_mode to clear count
                Action::SelectNext => ModeTransition::new_mode_and_dirty(
                    Normal::new(),
                    buffer.select_next(self.count_state.to_count()),
                ),
                Action::SelectPrev => ModeTransition::new_mode_and_dirty(
                    Normal::new(),
                    buffer.select_prev(self.count_state.to_count()),
                ),
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
                Action::Measure => ModeTransition::new_mode_and_info(
                    Normal::new(),
                    format!(
                        "{} = 0x{:x} bytes",
                        buffer.selection.main().len(),
                        buffer.selection.main().len()
                    ),
                ),
                Action::CommandMode => ModeTransition::new_mode(modes::command::Command::new()),
                Action::Undo => buffer.perform_undo().map_or_else(
                    || {
                        ModeTransition::new_mode_and_info(
                            Normal::new(),
                            "nothing left to undo".to_owned(),
                        )
                    },
                    |dirty| ModeTransition::new_mode_and_dirty(Normal::new(), dirty),
                ),
                Action::Redo => buffer.perform_redo().map_or_else(
                    || {
                        ModeTransition::new_mode_and_info(
                            Normal::new(),
                            "nothing left to redo".to_owned(),
                        )
                    },
                    |dirty| ModeTransition::new_mode_and_dirty(Normal::new(), dirty),
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

impl Normal {
    pub fn new() -> Normal {
        Normal {
            count_state: cmd_count::State::None,
        }
    }
}
