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
use super::view::view_options::{DirtyBytes, ViewOptions};

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
        self,
        event: &Event,
        buffers: &mut Buffers,
        options: &mut ViewOptions,
    ) -> ModeTransition {
        let buffer = buffers.current_mut();
        if let cmd_count::Transition::Update(new_state) = self.count_state.transition(event) {
            ModeTransition::new_mode(Normal {
                count_state: new_state,
            })
        } else if let Some(action) = DEFAULT_MAPS.event_to_action(event) {
            match action {
                Action::JumpToMode => match self.count_state {
                    cmd_count::State::None => {
                        ModeTransition::new_mode(modes::jumpto::JumpTo { extend: false })
                    }
                    cmd_count::State::Some { count: offset, .. } => {
                        options.make_dirty(
                            buffer.map_selections(|region| vec![region.jump_to(offset)]),
                        );
                        ModeTransition::new_mode(Normal::new())
                    }
                },
                Action::ExtendToMode => match self.count_state {
                    cmd_count::State::None => {
                        ModeTransition::new_mode(modes::jumpto::JumpTo { extend: true })
                    }
                    cmd_count::State::Some { count: offset, .. } => {
                        options.make_dirty(buffer.map_selections(|region| vec![region.extend_to(offset)]));
                        ModeTransition::new_mode(Normal::new())
                    }
                },
                Action::SplitMode => ModeTransition::new_mode(modes::split::Split::new()),
                Action::Insert { hex } => {
                    options.make_dirty(buffer.map_selections(|region| vec![region.to_backward()]));
                    ModeTransition::new_mode(modes::insert::Insert {
                        hex,
                        before: true,
                        hex_half: None,
                    })
                }
                Action::Append { hex } => {
                    let max_size = buffer.data.len();
                    options.make_dirty(buffer.map_selections(|region| {
                        vec![region.to_forward().simple_extend(
                            Direction::Right,
                            options.bytes_per_line,
                            max_size,
                            1,
                        )]
                    }));
                    ModeTransition::new_mode(modes::insert::Insert {
                        hex,
                        before: false,
                        hex_half: None,
                    })
                }
                Action::ReplaceMode { hex } => ModeTransition::new_mode(modes::replace::Replace {
                    hex,
                    hex_half: None,
                }),
                Action::Move(direction) => {
                    let max_bytes = buffer.data.len();
                    options.make_dirty(buffer.map_selections(|region| {
                        vec![region.simple_move(
                            direction,
                            options.bytes_per_line,
                            max_bytes,
                            self.count_state.to_count(),
                        )]
                    }));
                    ModeTransition::new_mode(Normal::new())
                }
                Action::Extend(direction) => {
                    let max_bytes = buffer.data.len();
                    options.make_dirty(buffer.map_selections(|region| {
                        vec![region.simple_extend(
                            direction,
                            options.bytes_per_line,
                            max_bytes,
                            self.count_state.to_count(),
                        )]
                    }));
                    ModeTransition::new_mode(Normal::new())
                }
                Action::SwapCaret => {
                    buffer.map_selections(|region| vec![region.swap_caret()]);
                    ModeTransition::new_mode(Normal::new())
                }
                Action::CollapseSelection => {
                    buffer.map_selections(|region| vec![region.collapse()]);
                    ModeTransition::new_mode(Normal::new())
                }
                Action::Delete { register } => {
                    buffer.yank_selections(register);
                    if !buffer.data.is_empty() {
                        let delta = ops::deletion(&buffer.data, &buffer.selection);
                        options.make_dirty(buffer.apply_delta(delta));
                    }
                    ModeTransition::new_mode(Normal::new())
                }
                Action::Change { hex, register } => {
                    buffer.yank_selections(register);
                    if !buffer.data.is_empty() {
                        let delta = ops::deletion(&buffer.data, &buffer.selection);
                        options.make_dirty(buffer.apply_delta(delta));
                    }
                    ModeTransition::new_mode(modes::insert::Insert {
                        hex,
                        before: true,
                        hex_half: None,
                    })
                }
                Action::Yank { register } => {
                    buffer.yank_selections(register);
                    ModeTransition::new_mode(Normal::new())
                }
                Action::Paste { register, after } => {
                    let delta = ops::paste(
                        &buffer.data,
                        &buffer.selection,
                        &buffer.registers.get(&register).unwrap_or(&vec![vec![]]),
                        after,
                        self.count_state.to_count(),
                    );
                    options.make_dirty(buffer.apply_delta(delta));
                    ModeTransition::new_mode(Normal::new())
                }
                // selection indexing in the UI starts at 1
                // hence we check for count > 0 and offset by -1
                Action::RemoveMain => {
                    match self.count_state {
                        cmd_count::State::Some { count, .. } if count > 0 => {
                            options.make_dirty(buffer.remove_selection(count - 1))
                        }

                        _ => options
                            .make_dirty(buffer.remove_selection(buffer.selection.main_selection)),
                    }
                    ModeTransition::new_mode(Normal::new())
                }
                Action::RetainMain => {
                    match self.count_state {
                        cmd_count::State::Some { count, .. } if count > 0 => {
                            options.make_dirty(buffer.retain_selection(count - 1))
                        }
                        _ => options
                            .make_dirty(buffer.retain_selection(buffer.selection.main_selection)),
                    }
                    ModeTransition::new_mode(Normal::new())
                }

                // new_mode to clear count
                Action::SelectNext => {
                    options.make_dirty(buffer.select_next(self.count_state.to_count()));
                    ModeTransition::new_mode(Normal::new())
                }
                Action::SelectPrev => {
                    options.make_dirty(buffer.select_prev(self.count_state.to_count()));
                    ModeTransition::new_mode(Normal::new())
                }
                Action::SelectAll => {
                    buffer.selection.select_all(buffer.data.len());
                    options.make_dirty(DirtyBytes::ChangeInPlace(vec![
                        (0..buffer.data.len()).into()
                    ]));
                    ModeTransition::new_mode(Normal::new())
                }
                Action::CollapseMode { hex } => ModeTransition::new_mode(
                    modes::search::Search::new(modes::collapse::Collapse(), hex),
                ),
                Action::Measure => {
                    options.info = Some(format!(
                        "{} = 0x{:x} bytes",
                        buffer.selection.main().len(),
                        buffer.selection.main().len()
                    ));
                    ModeTransition::new_mode(Normal::new())
                }
                Action::CommandMode => ModeTransition::new_mode(modes::command::Command::new()),
                Action::Undo => {
                    match buffer.perform_undo() {
                        None => options.info = Some("nothing left to undo".to_owned()),
                        Some(dirty) => options.make_dirty(dirty),
                    }
                    ModeTransition::new_mode(Normal::new())
                }
                Action::Undo => {
                    match buffer.perform_redo() {
                        None => options.info = Some("nothing left to redo".to_owned()),
                        Some(dirty) => options.make_dirty(dirty),
                    }
                    ModeTransition::new_mode(Normal::new())
                }
            }
        } else {
            ModeTransition::not_handled(self)
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
