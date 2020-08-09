use std::collections::HashMap;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use lazy_static::lazy_static;

use super::buffer::*;
use super::keymap::*;
use super::operations as ops;
use super::selection::Direction;
use super::state::*;

#[derive(Debug, PartialEq, Clone, Copy)]
enum Action {
    Quit,
    Move(Direction),
    Extend(Direction),
    SplitMode,
    JumpToMode,
    ExtendToMode,
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
            ('R' => Action::ReplaceMode{hex: false})
        ),
    }
}

lazy_static! {
    static ref DEFAULT_MAPS: KeyMap<Action> = default_maps();
}

pub fn transition(
    event: &Event,
    buffer: &mut Buffer,
    bytes_per_line: usize,
) -> Option<StateTransition> {
    if let Some(action) = DEFAULT_MAPS.event_to_action(event) {
        Some(match action {
            Action::Quit => StateTransition::NewState(State::Quitting),
            Action::JumpToMode => StateTransition::NewState(State::JumpTo { extend: false }),
            Action::ExtendToMode => StateTransition::NewState(State::JumpTo { extend: true }),
            Action::SplitMode => StateTransition::NewState(State::Split { count: None }),
            Action::Insert { hex } => StateTransition::StateAndDirtyBytes(
                State::Insert {
                    hex,
                    before: true,
                    hex_half: None,
                },
                buffer.map_selections(|region| vec![region.to_backward()]),
            ),
            Action::Append { hex } => StateTransition::StateAndDirtyBytes(
                State::Insert {
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
            Action::ReplaceMode { hex } => StateTransition::NewState(State::Replace {
                hex,
                hex_half: None,
            }),
            Action::Move(direction) => {
                let max_bytes = buffer.data.len();
                StateTransition::DirtyBytes(buffer.map_selections(|region| {
                    vec![region.simple_move(direction, bytes_per_line, max_bytes)]
                }))
            }
            Action::Extend(direction) => {
                let max_bytes = buffer.data.len();
                StateTransition::DirtyBytes(buffer.map_selections(|region| {
                    vec![region.simple_extend(direction, bytes_per_line, max_bytes)]
                }))
            }
            Action::SwapCaret => StateTransition::DirtyBytes(
                buffer.map_selections(|region| vec![region.swap_caret()]),
            ),
            Action::CollapseSelection => {
                StateTransition::DirtyBytes(buffer.map_selections(|region| vec![region.collapse()]))
            }
            Action::Delete { register } => {
                buffer.yank_selections(register);
                if !buffer.data.is_empty() {
                    let delta = ops::deletion(&buffer.data, &buffer.selection);
                    StateTransition::DirtyBytes(buffer.apply_delta(&delta))
                } else {
                    StateTransition::None
                }
            }
            Action::Change { hex, register } => {
                buffer.yank_selections(register);
                if !buffer.data.is_empty() {
                    let delta = ops::deletion(&buffer.data, &buffer.selection);
                    StateTransition::StateAndDirtyBytes(
                        State::Insert {
                            hex,
                            before: true,
                            hex_half: None,
                        },
                        buffer.apply_delta(&delta),
                    )
                } else {
                    StateTransition::NewState(State::Insert {
                        hex,
                        before: true,
                        hex_half: None,
                    })
                }
            }
            Action::Yank { register } => {
                buffer.yank_selections(register);
                StateTransition::None
            }
            Action::Paste { register, after } => {
                let delta = ops::paste(
                    &buffer.data,
                    &buffer.selection,
                    &buffer.registers.get(&register).unwrap_or(&vec![vec![]]),
                    after,
                );
                StateTransition::DirtyBytes(buffer.apply_delta(&delta))
            }
            Action::RemoveMain => StateTransition::DirtyBytes(buffer.remove_main_sel()),
            Action::RetainMain => StateTransition::DirtyBytes(buffer.retain_main_sel()),
            Action::SelectNext => StateTransition::DirtyBytes(buffer.select_next()),
            Action::SelectPrev => StateTransition::DirtyBytes(buffer.select_prev()),
            Action::SelectAll => {
                buffer.selection.select_all(buffer.data.len());
                StateTransition::DirtyBytes(DirtyBytes::ChangeInPlace(vec![
                    (0..buffer.data.len()).into()
                ]))
            }
        })
    } else {
        None
    }
}
