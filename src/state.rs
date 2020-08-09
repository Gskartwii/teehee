use std::borrow::Cow;
use xi_rope::Interval;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum State {
    Quitting,
    Normal,
    JumpTo {
        extend: bool,
    },
    Split {
        count: Option<usize>,
    },
    Insert {
        before: bool,
        hex: bool,
        hex_half: Option<u8>,
    },
    Replace {
        hex: bool,
        hex_half: Option<u8>,
    },
}

impl State {
    pub fn name(&self) -> Cow<str> {
        match self {
            State::Quitting => "QUIT".into(),
            State::Normal => "NORMAL".into(),
            State::JumpTo { extend: true } => "EXTEND".into(),
            State::JumpTo { extend: false } => "JUMP".into(),
            State::Split { count: None } => "SPLIT".into(),
            State::Split { count: Some(cnt) } => format!("SPLIT ({})", cnt).into(),
            State::Insert {
                before: true,
                hex: true,
                ..
            } => "INSERT (hex)".into(),
            State::Insert {
                before: true,
                hex: false,
                ..
            } => "INSERT (ascii)".into(),
            State::Insert {
                before: false,
                hex: true,
                ..
            } => "APPEND (hex)".into(),
            State::Insert {
                before: false,
                hex: false,
                ..
            } => "APPEND (ascii)".into(),
            State::Replace {
                hex: true,
                hex_half: None,
            } => "REPLACE (hex)".into(),
            State::Replace { hex: false, .. } => "REPLACE (ascii)".into(),
            State::Replace {
                hex: true,
                hex_half: Some(ch),
            } => format!("REPLACE (hex: {:x}...)", ch >> 4).into(),
        }
    }

    pub fn takes_input(&self) -> bool {
        self != &State::Quitting
    }

    pub fn has_half_cursor(&self) -> bool {
        match self {
            State::Insert {
                hex_half: Some(_), ..
            } => true,
            _ => false,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum DirtyBytes {
    ChangeInPlace(Vec<Interval>),
    ChangeLength,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum StateTransition {
    None,
    NewState(State),
    DirtyBytes(DirtyBytes),
    StateAndDirtyBytes(State, DirtyBytes),
}
