use crossterm::terminal;
use xi_rope::Interval;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum DirtyBytes {
    ChangeInPlace(Vec<Interval>),
    ChangeLength,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ViewOptions {
    pub size: (u16, u16),
    pub bytes_per_line: usize,
    pub start_offset: usize,
    pub info: Option<String>,
    pub dirty: Option<DirtyBytes>,
}

impl ViewOptions {
    pub fn new() -> ViewOptions {
        ViewOptions {
            bytes_per_line: 0x10,
            start_offset: 0,
            size: terminal::size().unwrap(),
            info: None,
            dirty: None,
        }
    }

    pub fn make_dirty(&mut self, new_dirty: DirtyBytes) {
        match self.dirty {
            Some(DirtyBytes::ChangeLength) => {},
            _ => self.dirty = Some(new_dirty),
        }
    }
}
