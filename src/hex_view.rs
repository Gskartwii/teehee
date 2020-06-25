use std::cmp;
use std::fmt;
use std::ops::Range;

use crossterm::{
    cursor, event,
    event::{Event, KeyCode},
    execute, queue, style, terminal, Result,
};

use std::io::{stdout, Write};

const VERTICAL: &str = "│";

#[derive(Debug, Clone, Copy)]
enum State {
    Quitting,
    Normal,
}

struct ByteAsciiRepr(u8);
impl fmt::Display for ByteAsciiRepr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.0.is_ascii_graphic() || self.0 == 0x20 {
            write!(f, "{}", char::from(self.0))
        } else {
            write!(f, ".")
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
struct Selection {
    cursor: usize,
    tail: usize,
}

pub struct HexView {
    data: Vec<u8>,
    size: (u16, u16),
    bytes_per_line: usize,
    start_offset: usize,
    selections: Vec<Range<usize>>,
    main_selection: usize,

    state: State,
}

impl HexView {
    pub fn from_data(data: Vec<u8>) -> HexView {
        HexView {
            data,
            bytes_per_line: 0x10,
            start_offset: 0,
            selections: vec![0..1],
            main_selection: 0,
            size: terminal::size().unwrap(),

            state: State::Normal,
        }
    }

    pub fn set_bytes_per_line(&mut self, bpl: usize) {
        self.bytes_per_line = bpl;
    }

    fn hex_digits_in_offset(&self) -> usize {
        let bits_in_offset = (32 - (self.data.len() as u32).leading_zeros()) as usize;
        let hex_digits_in_offset = (bits_in_offset + 3) / 4;

        hex_digits_in_offset
    }

    fn draw_hex_row(&self, stdout: &mut impl Write, bytes: &[u8]) -> Result<()> {
        for byte in bytes.iter().copied() {
            queue!(stdout, style::Print(format!("{:02x} ", byte)))?;
        }
        Ok(())
    }

    fn draw_ascii_row(&self, stdout: &mut impl Write, bytes: &[u8]) -> Result<()> {
        let bytes_per_line = self.bytes_per_line;
        let hex_digits_in_offset = self.hex_digits_in_offset();

        for byte in bytes.iter().copied() {
            queue!(stdout, style::Print(format!("{}", ByteAsciiRepr(byte))))?;
        }
        Ok(())
    }

    fn draw_offset(&self, stdout: &mut impl Write, offset: usize, digits: usize) -> Result<()> {
        queue!(stdout, style::Print(format!("{:>1$x}", offset, digits)))
    }

    fn draw_separator(&self, stdout: &mut impl Write) -> Result<()> {
        queue!(stdout, style::Print(format!(" {} ", VERTICAL)))
    }

    fn offset_to_row(&self, offset: usize) -> u16 {
        debug_assert!(offset >= self.start_offset);
        ((offset - self.start_offset) / self.bytes_per_line) as u16
    }

    fn draw_row(
        &self,
        stdout: &mut impl Write,
        offset: usize,
        digits_in_offset: usize,
    ) -> Result<()> {
        let end = cmp::min(self.data.len(), offset + self.bytes_per_line);
        let bytes = &self.data[offset..end];

        queue!(
            stdout,
            cursor::MoveTo(0, self.offset_to_row(offset)),
            style::Print(" ".to_string()), // Padding
        )?;
        self.draw_offset(stdout, offset, digits_in_offset)?;
        self.draw_separator(stdout)?;
        self.draw_hex_row(stdout, bytes)?;
        queue!(
            stdout,
            cursor::MoveTo(
                (1 + digits_in_offset + 3 + 3 * self.bytes_per_line - 1) as u16,
                self.offset_to_row(offset),
            ),
        )?;
        self.draw_separator(stdout)?;
        self.draw_ascii_row(stdout, bytes)?;
        Ok(())
    }

    fn visible_bytes(&self) -> Range<usize> {
        self.start_offset
            ..cmp::min(
                self.data.len(),
                self.start_offset + self.size.1 as usize * self.bytes_per_line,
            )
    }

    fn draw(&self, stdout: &mut impl Write) -> Result<()> {
        queue!(stdout, terminal::Clear(terminal::ClearType::All))?;
        let digits_in_offset = self.hex_digits_in_offset();
        for i in self.visible_bytes().step_by(self.bytes_per_line) {
            self.draw_row(stdout, i, digits_in_offset)?;
        }

        stdout.flush()?;
        Ok(())
    }

    pub fn run_event_loop(mut self, stdout: &mut impl Write) -> Result<()> {
        execute!(stdout, terminal::EnterAlternateScreen)?;

        self.draw(stdout)?;

        terminal::enable_raw_mode()?;
        loop {
            match self.state {
                State::Quitting => break,
                State::Normal => match event::read()? {
                    Event::Resize(x, y) => {
                        self.size = (x, y);
                        self.draw(stdout)?;
                    }
                    Event::Key(event) if event.code == KeyCode::Esc => {
                        self.state = State::Quitting;
                    }
                    _ => {}
                },
            }
        }
        execute!(stdout, terminal::LeaveAlternateScreen)?;
        terminal::disable_raw_mode()?;
        Ok(())
    }
}
