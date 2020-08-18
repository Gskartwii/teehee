use std::collections::HashMap;

use super::keymap::KeyMap;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use lazy_static::lazy_static;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum State {
    None,
    Some { hex: bool, count: usize },
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum Action {
    AppendDigit(u8),
    CancelEntry,
    SwitchHexEntry,
    RemoveLast,
}

fn default_maps() -> KeyMap<Action> {
    KeyMap {
        maps: keys!(
             (key KeyCode::Esc => Action::CancelEntry),
             (key KeyCode::Backspace => Action::RemoveLast),
             ('x' => Action::SwitchHexEntry),
             ('0' => Action::AppendDigit(0)),
             ('1' => Action::AppendDigit(1)),
             ('2' => Action::AppendDigit(2)),
             ('3' => Action::AppendDigit(3)),
             ('4' => Action::AppendDigit(4)),
             ('5' => Action::AppendDigit(5)),
             ('6' => Action::AppendDigit(6)),
             ('7' => Action::AppendDigit(7)),
             ('8' => Action::AppendDigit(8)),
             ('9' => Action::AppendDigit(9)),
             ('a' => Action::AppendDigit(10)),
             ('b' => Action::AppendDigit(11)),
             ('c' => Action::AppendDigit(12)),
             ('d' => Action::AppendDigit(13)),
             ('e' => Action::AppendDigit(14)),
             ('f' => Action::AppendDigit(15))
        ),
    }
}

lazy_static! {
    static ref DEFAULT_MAPS: KeyMap<Action> = default_maps();
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum Transition {
    NotHandled,
    Update(State),
}

impl State {
    pub fn transition(self, event: &Event) -> Transition {
        if let Some(action) = DEFAULT_MAPS.event_to_action(event) {
            match (self, action) {
                (State::None, Action::AppendDigit(d)) if d > 9 => Transition::NotHandled,
                (State::None, Action::AppendDigit(d)) => Transition::Update(State::Some {
                    hex: false,
                    count: d as usize,
                }),
                (State::None, Action::CancelEntry) => Transition::NotHandled,
                (State::None, Action::SwitchHexEntry) => Transition::Update(State::Some {
                    hex: true,
                    count: 0,
                }),
                (State::None, Action::RemoveLast) => Transition::NotHandled,
                (State::Some { hex: false, .. }, Action::AppendDigit(d)) if d > 9 => {
                    // abcdef should not be handled unless in hex mode
                    Transition::NotHandled
                }
                (State::Some { hex: true, count }, Action::AppendDigit(d)) => {
                    Transition::Update(State::Some {
                        count: count << 4 | d as usize,
                        hex: true,
                    })
                }
                (State::Some { hex: false, count }, Action::AppendDigit(d)) => {
                    Transition::Update(State::Some {
                        count: count * 10 + d as usize,
                        hex: false,
                    })
                }
                (State::Some { hex: true, count }, Action::RemoveLast) if count >= 0x10 => {
                    Transition::Update(State::Some {
                        count: count >> 4,
                        hex: true,
                    })
                }
                (State::Some { hex: true, .. }, Action::RemoveLast) => {
                    // count doesn't have double-digits in hex: reset
                    Transition::Update(State::None)
                }
                (State::Some { hex: false, count }, Action::RemoveLast) if count >= 10 => {
                    Transition::Update(State::Some {
                        count: count / 10,
                        hex: false,
                    })
                }
                (State::Some { hex: false, .. }, Action::RemoveLast) => {
                    // count doesn't have double-digits: reset
                    Transition::Update(State::None)
                }
                (State::Some { .. }, Action::CancelEntry) => Transition::Update(State::None),
                (State::Some { count, hex }, Action::SwitchHexEntry) => {
                    Transition::Update(State::Some { count, hex: !hex })
                }
            }
        } else {
            Transition::NotHandled
        }
    }
    pub fn to_count(&self) -> usize {
        match self {
            State::Some { count, .. } => *count,
            State::None => 1,
        }
    }
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            State::None => Ok(()),
            State::Some {
                hex: true,
                count: 0,
            } => write!(f, " (0x)"),
            State::Some { hex: true, count } => write!(f, " (0x{:x})", count),
            State::Some { hex: false, count } => write!(f, " ({})", count),
        }
    }
}
