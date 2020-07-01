use std::cmp;
use std::collections::HashSet;
use std::fmt;
use std::ops::Range;

use crossterm::{
    cursor, event,
    event::{Event, KeyCode},
    execute, queue, style, terminal, Result,
};

use super::selection::*;
use std::io::{stdout, Write};

const VERTICAL: &str = "│";

#[derive(Debug, Clone, Copy)]
enum State {
    Quitting,
    Normal,
}

#[derive(Debug, Clone, Copy)]
enum Priority {
    Basic,
    Mark,
    Selection,
    Cursor,
}

#[derive(Debug, Clone)]
struct PrioritizedStyle {
    style: style::ContentStyle,
    priority: Priority,
}

#[derive(Debug, Clone)]
enum StylingCommand {
    Start(PrioritizedStyle),
    End(PrioritizedStyle),
    StartEnd(PrioritizedStyle, PrioritizedStyle),
    None,
}

impl StylingCommand {
    fn start_style(&self) -> Option<&style::ContentStyle> {
        match self {
            StylingCommand::Start(PrioritizedStyle { style, .. }) => Some(style),
            StylingCommand::StartEnd(PrioritizedStyle { style, .. }, _) => Some(style),
            _ => None,
        }
    }
    fn end_style(&self) -> Option<&style::ContentStyle> {
        match self {
            StylingCommand::End(PrioritizedStyle { style, .. }) => Some(style),
            StylingCommand::StartEnd(_, PrioritizedStyle { style, .. }) => Some(style),
            _ => None,
        }
    }
    fn with_start_style(self, style: PrioritizedStyle) -> StylingCommand {
        match self {
            StylingCommand::None | StylingCommand::Start(_) => StylingCommand::Start(style),
            StylingCommand::End(e) | StylingCommand::StartEnd(_, e) => {
                StylingCommand::StartEnd(style, e)
            }
        }
    }
    fn with_end_style(self, style: PrioritizedStyle) -> StylingCommand {
        match self {
            StylingCommand::None | StylingCommand::End(_) => StylingCommand::End(style),
            StylingCommand::Start(s) | StylingCommand::StartEnd(s, _) => {
                StylingCommand::StartEnd(s, style)
            }
        }
    }
}

fn queue_style(stdout: &mut impl Write, style: &style::ContentStyle) -> Result<()> {
    if let Some(fg) = style.foreground_color {
        queue!(stdout, style::SetForegroundColor(fg))?;
    }
    if let Some(bg) = style.background_color {
        queue!(stdout, style::SetBackgroundColor(bg))?;
    }
    if !style.attributes.is_empty() {
        queue!(stdout, style::SetAttributes(style.attributes))?;
    }
    Ok(())
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

pub struct HexView {
    data: Vec<u8>,
    size: (u16, u16),
    bytes_per_line: usize,
    start_offset: usize,
    selection: Selection,

    state: State,
}

impl HexView {
    pub fn from_data(data: Vec<u8>) -> HexView {
        HexView {
            data,
            bytes_per_line: 0x10,
            start_offset: 0,
            size: terminal::size().unwrap(),
            selection: Selection::new(),

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

    fn draw_hex_row(
        &self,
        stdout: &mut impl Write,
        styled_bytes: impl IntoIterator<Item = (u8, StylingCommand)>,
    ) -> Result<()> {
        for (byte, style_cmd) in styled_bytes.into_iter() {
            if let Some(start_cmd) = style_cmd.start_style() {
                queue_style(stdout, start_cmd)?;
            }
            queue!(stdout, style::Print(format!("{:02x}", byte)))?;
            if let Some(end_cmd) = style_cmd.end_style() {
                queue_style(stdout, end_cmd)?;
            }
            queue!(stdout, style::Print(format!(" ")))?;
        }
        Ok(())
    }

    fn draw_ascii_row(
        &self,
        stdout: &mut impl Write,
        styled_bytes: impl IntoIterator<Item = (u8, StylingCommand)>,
    ) -> Result<()> {
        let bytes_per_line = self.bytes_per_line;
        let hex_digits_in_offset = self.hex_digits_in_offset();

        for (byte, style_cmd) in styled_bytes.into_iter() {
            if let Some(start_cmd) = style_cmd.start_style() {
                queue_style(stdout, start_cmd);
            }
            queue!(stdout, style::Print(format!("{}", ByteAsciiRepr(byte))))?;
            if let Some(end_cmd) = style_cmd.end_style() {
                queue_style(stdout, end_cmd);
            }
        }
        Ok(())
    }

    fn draw_offset(&self, stdout: &mut impl Write, offset: usize, digits: usize) -> Result<()> {
        queue!(stdout, style::Print(format!("{:>1$x}", offset, digits)))
    }

    fn draw_separator(&self, stdout: &mut impl Write) -> Result<()> {
        queue!(stdout, style::Print(format!(" {} ", VERTICAL)))
    }

    fn offset_to_row(&self, offset: usize) -> Option<u16> {
        if offset < self.start_offset {
            return None;
        }
        let normalized_offset = offset - self.start_offset;
        let bytes_per_line = self.bytes_per_line;
        let max_bytes = bytes_per_line * self.size.1 as usize;
        if normalized_offset > max_bytes {
            return None;
        }
        Some((normalized_offset / bytes_per_line) as u16)
    }

    fn draw_row(
        &self,
        stdout: &mut impl Write,
        offset: usize,
        digits_in_offset: usize,
        mark_commands: &[StylingCommand],
    ) -> Result<()> {
        let end = cmp::min(self.data.len(), offset + self.bytes_per_line);
        let bytes = &self.data[offset..end];
        let row_num = self.offset_to_row(offset).unwrap();

        queue!(
            stdout,
            cursor::MoveTo(0, row_num),
            style::Print(" ".to_string()), // Padding
        )?;
        self.draw_offset(stdout, offset, digits_in_offset)?;
        self.draw_separator(stdout)?;
        self.draw_hex_row(
            stdout,
            bytes.iter().copied().zip(mark_commands.iter().cloned()),
        )?;
        queue!(
            stdout,
            cursor::MoveTo(
                (1 + digits_in_offset + 3 + 3 * self.bytes_per_line - 1) as u16,
                row_num,
            ),
        )?;
        self.draw_separator(stdout)?;
        self.draw_ascii_row(
            stdout,
            bytes.iter().copied().zip(mark_commands.iter().cloned()),
        )?;
        Ok(())
    }

    fn visible_bytes(&self) -> Range<usize> {
        self.start_offset
            ..cmp::min(
                self.data.len(),
                self.start_offset + self.size.1 as usize * self.bytes_per_line,
            )
    }

    fn default_style(&self) -> PrioritizedStyle {
        PrioritizedStyle {
            style: style::ContentStyle::new()
                .foreground(style::Color::White)
                .background(style::Color::Black),
            priority: Priority::Basic,
        }
    }
    fn selection_style(&self) -> PrioritizedStyle {
        PrioritizedStyle {
            style: style::ContentStyle::new()
                .foreground(style::Color::Grey)
                .background(style::Color::DarkYellow),
            priority: Priority::Selection,
        }
    }
    fn caret_style(&self) -> PrioritizedStyle {
        PrioritizedStyle {
            style: style::ContentStyle::new()
                .foreground(style::Color::AnsiValue(16))
                .background(style::Color::White),
            priority: Priority::Cursor,
        }
    }

    fn mark_commands(&self, visible: Range<usize>) -> Vec<StylingCommand> {
        let mut mark_commands = vec![StylingCommand::None; visible.len()];
        let mut command_stack = vec![self.default_style()];
        let mut selected_regions = self.selection.regions_in_range(visible.start, visible.end);

        let start = visible.start;
        for i in visible {
            let normalized = i - start;
            if !selected_regions.is_empty() {
                if selected_regions[0].start == i {
                    command_stack.push(self.selection_style());
                    mark_commands[normalized] = mark_commands[normalized]
                        .clone()
                        .with_start_style(command_stack.last().unwrap().clone());
                }
                if selected_regions[0].caret() == i {
                    let base_style = command_stack.last().unwrap().clone();
                    mark_commands[normalized] = mark_commands[normalized]
                        .clone()
                        .with_start_style(self.caret_style())
                        .with_end_style(base_style);
                }
                if selected_regions[0].end == i + 1 {
                    command_stack.pop();
                    selected_regions = &selected_regions[1..];
                    mark_commands[normalized] = mark_commands[normalized]
                        .clone()
                        .with_end_style(command_stack.last().unwrap().clone());
                }
            }

            if i % self.bytes_per_line == 0 && mark_commands[normalized].start_style().is_none() {
                // line starts: restore applied style
                mark_commands[normalized] = mark_commands[normalized]
                    .clone()
                    .with_start_style(command_stack.last().unwrap().clone());
            } else if (i + 1) % self.bytes_per_line == 0 {
                // line ends: apply default style
                mark_commands[normalized] = mark_commands[normalized]
                    .clone()
                    .with_end_style(self.default_style());
            }
        }

        mark_commands
    }

    fn draw_rows(&self, stdout: &mut impl Write, invalidated_rows: &HashSet<u16>) -> Result<()> {
        let digits_in_offset = self.hex_digits_in_offset();
        let visible_bytes = self.visible_bytes();
        let max_bytes = visible_bytes.len();
        let mark_commands = self.mark_commands(visible_bytes.clone());

        for i in visible_bytes.step_by(self.bytes_per_line) {
            if !invalidated_rows.contains(&self.offset_to_row(i).unwrap()) {
                continue;
            }
            self.draw_row(
                stdout,
                i,
                digits_in_offset,
                &mark_commands[i..std::cmp::min(max_bytes, i + self.bytes_per_line)],
            )?;
        }

        stdout.flush()?;
        Ok(())
    }

    fn draw(&self, stdout: &mut impl Write) -> Result<()> {
        queue!(stdout, terminal::Clear(terminal::ClearType::All))?;
        let digits_in_offset = self.hex_digits_in_offset();
        let visible_bytes = self.visible_bytes();
        let max_bytes = visible_bytes.len();
        let mark_commands = self.mark_commands(visible_bytes.clone());

        for i in visible_bytes.step_by(self.bytes_per_line) {
            self.draw_row(
                stdout,
                i,
                digits_in_offset,
                &mark_commands[i..std::cmp::min(max_bytes, i + self.bytes_per_line)],
            )?;
        }

        stdout.flush()?;
        Ok(())
    }

    pub fn run_event_loop(mut self, stdout: &mut impl Write) -> Result<()> {
        execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;

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
                    Event::Key(event) if event.code == KeyCode::Char('l') => {
                        let max_bytes = self.data.len();
                        let bytes_per_line = self.bytes_per_line;
                        let mut invalidated_ranges = Vec::new();
                        self.selection.map_selections(|region| {
                            invalidated_ranges.push(region.start..region.end);
                            let new =
                                region.simple_move(Direction::Right, bytes_per_line, max_bytes);
                            invalidated_ranges.push(new.start..new.end);
                            new
                        });
                        let mut invalidated_rows = HashSet::new();
                        for offset in invalidated_ranges.into_iter().flatten() {
                            if let Some(invalidated_row) = self.offset_to_row(offset) {
                                invalidated_rows.insert(invalidated_row);
                            }
                        }
                        self.draw_rows(stdout, &invalidated_rows)?;
                    }
                    _ => {}
                },
            }
        }
        execute!(stdout, cursor::Show, terminal::LeaveAlternateScreen)?;
        terminal::disable_raw_mode()?;
        Ok(())
    }
}
