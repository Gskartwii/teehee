use std::cell::Cell;
use std::cmp;
use std::collections::HashSet;
use std::fmt;
use std::ops::Range;
use std::time;

use crossterm::{
    cursor,
    event::{self, Event},
    execute, queue, style,
    style::Color,
    terminal, Result,
};
use xi_rope::Interval;

use super::buffer::*;
use super::mode::*;
use super::modes;
use std::io::Write;

const VERTICAL: &str = "│";
const LEFTARROW: &str = "";

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

#[derive(Debug, Clone, Default)]
struct StylingCommand {
    start: Option<PrioritizedStyle>,
    mid: Option<PrioritizedStyle>,
    end: Option<PrioritizedStyle>,
}

impl StylingCommand {
    fn start_style(&self) -> Option<&style::ContentStyle> {
        self.start.as_ref().map(|x| &x.style)
    }
    fn mid_style(&self) -> Option<&style::ContentStyle> {
        self.mid.as_ref().map(|x| &x.style)
    }
    fn end_style(&self) -> Option<&style::ContentStyle> {
        self.end.as_ref().map(|x| &x.style)
    }
    fn with_start_style(self, style: PrioritizedStyle) -> StylingCommand {
        StylingCommand {
            start: Some(style),
            ..self
        }
    }
    fn with_mid_style(self, style: PrioritizedStyle) -> StylingCommand {
        StylingCommand {
            mid: Some(style),
            ..self
        }
    }
    fn with_end_style(self, style: PrioritizedStyle) -> StylingCommand {
        StylingCommand {
            end: Some(style),
            ..self
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

fn make_padding(len: usize) -> &'static str {
    debug_assert!(len < 0x40, "can't make padding of len {}", len);
    &"                                                                "[..len]
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

struct MixedRepr(u8);
impl fmt::Display for MixedRepr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.0.is_ascii_graphic() || self.0 == 0x20 {
            write!(f, "{}", char::from(self.0))
        } else {
            write!(f, "<{:2x}>", self.0)
        }
    }
}

trait StatusLinePrompter: Mode {
    fn render_with_size(
        &self,
        stdout: &mut dyn Write,
        max_width: usize,
        last_start_col: usize,
    ) -> Result<usize>;
}

impl StatusLinePrompter for modes::search::Search {
    fn render_with_size(
        &self,
        stdout: &mut dyn Write,
        mut max_width: usize,
        last_start_col: usize,
    ) -> Result<usize> {
        let mut start_column = last_start_col;
        queue!(
            stdout,
            style::PrintStyledContent(
                style::style("search:")
                    .with(style::Color::White)
                    .on(style::Color::Blue),
            )
        )?;
        max_width -= "search:".len();

        if self.hex {
            let full_bytes = std::cmp::min(self.pattern.pieces.len(), max_width / 3);

            if self.pattern.pieces.len() <= full_bytes {
                start_column = 0;
            } else if self.cursor >= start_column + full_bytes {
                start_column = self.cursor - full_bytes;
            } else if self.cursor < start_column {
                start_column = self.cursor;
            }

            let normalized_cursor = self.cursor - start_column;
            for (i, piece) in self.pattern.pieces[start_column..full_bytes]
                .iter()
                .enumerate()
            {
                use modes::search::PatternPiece;
                match piece {
                    PatternPiece::Literal(byte) if normalized_cursor != i => {
                        queue!(stdout, style::Print(format!("{:2x} ", byte)))?
                    }
                    PatternPiece::Literal(byte)
                        if normalized_cursor == i && self.hex_half.is_some() =>
                    {
                        queue!(
                            stdout,
                            style::Print(format!("{:x}", byte >> 4)),
                            style::PrintStyledContent(
                                style::style(format!("{:x}", byte & 0xf))
                                    .with(style::Color::Black)
                                    .on(style::Color::White)
                            ),
                            style::Print(" "),
                        )?
                    }
                    PatternPiece::Literal(byte) => queue!(
                        stdout,
                        style::PrintStyledContent(
                            style::style(format!("{:2x}", byte))
                                .with(style::Color::Black)
                                .on(style::Color::White)
                        ),
                        style::Print(" "),
                    )?,
                    PatternPiece::Wildcard if normalized_cursor != i => queue!(
                        stdout,
                        style::PrintStyledContent(style::style("** ").with(style::Color::DarkRed))
                    )?,
                    PatternPiece::Wildcard => queue!(
                        stdout,
                        style::PrintStyledContent(
                            style::style("**")
                                .with(style::Color::DarkRed)
                                .on(style::Color::White)
                        ),
                        style::Print(" "),
                    )?,
                }
            }
            if self.cursor == self.pattern.pieces.len() {
                queue!(
                    stdout,
                    style::PrintStyledContent(
                        style::style("  ")
                            .with(style::Color::Black)
                            .on(style::Color::White)
                    ),
                    style::Print(" "),
                )?
            }

            return Ok(start_column);
        }
        queue!(
            stdout,
            style::PrintStyledContent(
                style::style("todo")
                    .with(style::Color::White)
                    .on(style::Color::Blue),
            )
        )?;
        return Ok(start_column);

        /*let mut remaining_width = max_width;
        let mut styled_bytes = vec![];

        Ok(start_column)*/
    }
}

pub struct HexView {
    buffer: Buffer,
    size: (u16, u16),
    bytes_per_line: usize,
    start_offset: usize,
    last_visible_rows: Cell<usize>,
    last_visible_prompt_col: Cell<usize>,
    last_draw_time: time::Duration,

    mode: Box<dyn Mode>,
}

impl HexView {
    pub fn from_data(data: Vec<u8>) -> HexView {
        HexView {
            buffer: Buffer::from_data(data),
            bytes_per_line: 0x10,
            start_offset: 0,
            size: terminal::size().unwrap(),
            last_visible_rows: Cell::new(0),
            last_visible_prompt_col: Cell::new(0),

            last_draw_time: Default::default(),

            mode: Box::new(modes::normal::Normal()),
        }
    }

    pub fn set_bytes_per_line(&mut self, bpl: usize) {
        self.bytes_per_line = bpl;
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
            queue!(stdout, style::Print(format!("{:x}", byte >> 4)))?;
            if let Some(mid_cmd) = style_cmd.mid_style() {
                queue_style(stdout, mid_cmd)?;
            }
            queue!(stdout, style::Print(format!("{:x}", byte & 0xf)))?;
            if let Some(end_cmd) = style_cmd.end_style() {
                queue_style(stdout, end_cmd)?;
            }
            queue!(stdout, style::Print(" ".to_string()))?;
        }
        Ok(())
    }

    fn draw_ascii_row(
        &self,
        stdout: &mut impl Write,
        styled_bytes: impl IntoIterator<Item = (u8, StylingCommand)>,
    ) -> Result<()> {
        for (byte, style_cmd) in styled_bytes.into_iter() {
            if let Some(start_cmd) = style_cmd.start_style() {
                queue_style(stdout, start_cmd)?;
            }
            queue!(stdout, style::Print(format!("{}", ByteAsciiRepr(byte))))?;
            if let Some(end_cmd) = style_cmd.end_style() {
                queue_style(stdout, end_cmd)?;
            }
        }
        Ok(())
    }

    fn draw_separator(&self, stdout: &mut impl Write) -> Result<()> {
        queue!(stdout, style::Print(format!("{} ", VERTICAL)))
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
        bytes: &[u8],
        offset: usize,
        mark_commands: &[StylingCommand],
    ) -> Result<()> {
        let row_num = self.offset_to_row(offset).unwrap();

        queue!(stdout, cursor::MoveTo(0, row_num))?;
        queue!(
            stdout,
            style::Print(" ".to_string()), // Padding
        )?;
        self.draw_hex_row(
            stdout,
            bytes.iter().copied().zip(mark_commands.iter().cloned()),
        )?;
        queue!(
            stdout,
            style::Print(make_padding(
                (self.bytes_per_line - bytes.len()) % self.bytes_per_line * 3
            )),
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
                self.buffer.data.len(),
                self.start_offset + (self.size.1 - 1) as usize * self.bytes_per_line,
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
    fn active_selection_style(&self) -> PrioritizedStyle {
        PrioritizedStyle {
            style: style::ContentStyle::new()
                .foreground(style::Color::Black)
                .background(style::Color::DarkYellow),
            priority: Priority::Selection,
        }
    }
    fn inactive_selection_style(&self) -> PrioritizedStyle {
        PrioritizedStyle {
            style: style::ContentStyle::new()
                .foreground(style::Color::Black)
                .background(style::Color::DarkGrey),
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
    fn empty_caret_style(&self) -> PrioritizedStyle {
        PrioritizedStyle {
            style: style::ContentStyle::new().background(style::Color::Green),
            priority: Priority::Cursor,
        }
    }

    fn mark_commands(&self, visible: Range<usize>) -> Vec<StylingCommand> {
        let mut mark_commands = vec![StylingCommand::default(); visible.len()];
        let mut selected_regions = self
            .buffer
            .selection
            .regions_in_range(visible.start, visible.end);
        let mut command_stack = vec![self.default_style()];
        let start = visible.start;

        // Add to command stack those commands that being out of bounds
        if !selected_regions.is_empty() && selected_regions[0].min() < start {
            command_stack.push(if selected_regions[0].is_main() {
                self.active_selection_style()
            } else {
                self.inactive_selection_style()
            });
        }

        for i in visible {
            let normalized = i - start;
            if !selected_regions.is_empty() {
                if selected_regions[0].min() == i {
                    command_stack.push(if selected_regions[0].is_main() {
                        self.active_selection_style()
                    } else {
                        self.inactive_selection_style()
                    });
                    mark_commands[normalized] = mark_commands[normalized]
                        .clone()
                        .with_start_style(command_stack.last().unwrap().clone());
                }
                if selected_regions[0].caret == i {
                    let base_style = command_stack.last().unwrap().clone();
                    let mut caret_cmd = mark_commands[normalized].clone();
                    if self.mode.has_half_cursor() {
                        if i == selected_regions[0].min() {
                            caret_cmd = caret_cmd
                                .with_mid_style(self.caret_style())
                                .with_end_style(base_style);
                        } else {
                            caret_cmd = caret_cmd
                                .with_start_style(base_style)
                                .with_mid_style(self.caret_style());
                        }
                    } else {
                        caret_cmd = caret_cmd
                            .with_start_style(self.caret_style())
                            .with_end_style(base_style);
                    }
                    mark_commands[normalized] = caret_cmd;
                }
                if selected_regions[0].max() == i {
                    mark_commands[normalized] = mark_commands[normalized]
                        .clone()
                        .with_end_style(command_stack[command_stack.len() - 2].clone());
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

            if !selected_regions.is_empty() && selected_regions[0].max() == i {
                // Must be popped after line config
                command_stack.pop();
                selected_regions = &selected_regions[1..];
            }
        }

        mark_commands
    }

    fn calculate_powerline_length(&self) -> usize {
        let mut length = 0;
        length += 1; // leftarrow
        length += 2 + self.mode.name().len();
        length += 1; // leftarrow
        length += format!(
            " {} sels ({}) ",
            self.buffer.selection.len(),
            self.buffer.selection.main_selection + 1
        )
        .len();
        length += 1; // leftarrow
        if !self.buffer.data.is_empty() {
            length += format!(
                " {:x}/{:x} ",
                self.buffer.selection.main_cursor_offset(),
                self.buffer.data.len() - 1
            )
            .len();
        } else {
            length += " empty ".len();
        }
        length
    }

    fn draw_statusline_here(&self, stdout: &mut impl Write) -> Result<()> {
        queue!(
            stdout,
            style::PrintStyledContent(style::style(LEFTARROW).with(Color::DarkYellow)),
            style::PrintStyledContent(
                style::style(format!(" {} ", self.mode.name()))
                    .with(Color::AnsiValue(16))
                    .on(Color::DarkYellow)
            ),
            style::PrintStyledContent(
                style::style(LEFTARROW)
                    .with(Color::White)
                    .on(Color::DarkYellow)
            ),
            style::PrintStyledContent(
                style::style(format!(
                    " {} sels ({}) ",
                    self.buffer.selection.len(),
                    self.buffer.selection.main_selection + 1
                ))
                .with(Color::AnsiValue(16))
                .on(Color::White)
            ),
        )?;
        if !self.buffer.data.is_empty() {
            queue!(
                stdout,
                style::PrintStyledContent(
                    style::style(LEFTARROW).with(Color::Blue).on(Color::White)
                ),
                style::PrintStyledContent(
                    style::style(format!(
                        " {:x}/{:x} ",
                        self.buffer.selection.main_cursor_offset(),
                        self.buffer.data.len() - 1,
                    ))
                    .with(Color::White)
                    .on(Color::Blue),
                ),
            )?;
        } else {
            queue!(
                stdout,
                style::PrintStyledContent(
                    style::style(LEFTARROW).with(Color::Blue).on(Color::White)
                ),
                style::PrintStyledContent(
                    style::style(" empty ").with(Color::White).on(Color::Blue),
                ),
            )?;
        }
        Ok(())
    }

    fn draw_statusline(&self, stdout: &mut impl Write) -> Result<()> {
        let line_length = self.calculate_powerline_length();
        queue!(
            stdout,
            cursor::MoveTo(self.size.0 - line_length as u16, self.size.1),
            terminal::Clear(terminal::ClearType::CurrentLine),
        )?;

        self.draw_statusline_here(stdout)?;
        if let Some(statusliner) = self.mode.as_any().downcast_ref::<modes::search::Search>() {
            queue!(stdout, cursor::MoveTo(0, self.size.1),)?;
            let prev_col = self.last_visible_prompt_col.get();
            let new_col = statusliner.render_with_size(stdout, self.size.0 as usize, prev_col)?;
            self.last_visible_prompt_col.set(new_col);
        }

        Ok(())
    }

    fn draw_empty(&self, stdout: &mut impl Write) -> Result<()> {
        queue!(stdout, cursor::MoveTo(0, 0), style::Print(" ".to_string()),)?;

        queue_style(stdout, &self.empty_caret_style().style)?;
        queue!(stdout, style::Print("  "))?;
        queue_style(stdout, &self.default_style().style)?;

        queue!(
            stdout,
            style::Print(make_padding(
                (self.bytes_per_line - 1) % self.bytes_per_line * 3
            )),
        )?;
        self.draw_separator(stdout)?;
        queue_style(stdout, &self.empty_caret_style().style)?;
        queue!(stdout, style::Print(" "))?;
        queue_style(stdout, &self.default_style().style)?;

        let new_full_rows = 0;
        if new_full_rows != self.last_visible_rows.get() {
            queue!(stdout, terminal::Clear(terminal::ClearType::FromCursorDown))?;
            self.last_visible_rows.set(new_full_rows);
        }

        Ok(())
    }

    fn draw_rows(&self, stdout: &mut impl Write, invalidated_rows: &HashSet<u16>) -> Result<()> {
        if self.buffer.data.is_empty() {
            self.draw_empty(stdout)?;
            return Ok(());
        }
        let visible_bytes = self.visible_bytes();
        let start_index = visible_bytes.start;
        let end_index = visible_bytes.end;

        let visible_bytes_cow = self.buffer.data.slice_to_cow(start_index..end_index);

        let max_bytes = end_index - start_index;
        let mark_commands = self.mark_commands(visible_bytes.clone());

        for i in visible_bytes.step_by(self.bytes_per_line) {
            if !invalidated_rows.contains(&self.offset_to_row(i).unwrap()) {
                continue;
            }

            let normalized_i = i - start_index;
            let normalized_end = std::cmp::min(max_bytes, normalized_i + self.bytes_per_line);
            self.draw_row(
                stdout,
                &visible_bytes_cow[normalized_i..normalized_end],
                i,
                &mark_commands[normalized_i..normalized_end],
            )?;
        }

        Ok(())
    }

    fn draw(&self, stdout: &mut impl Write) -> Result<time::Duration> {
        let begin = time::Instant::now();
        if self.buffer.data.is_empty() {
            self.draw_empty(stdout)?;
            return Ok(begin.elapsed());
        }

        queue!(stdout, cursor::MoveTo(0, 0))?;

        let visible_bytes = self.visible_bytes();
        let start_index = visible_bytes.start;
        let end_index = visible_bytes.end;
        let visible_bytes_cow = self.buffer.data.slice_to_cow(start_index..end_index);

        let max_bytes = end_index - start_index;
        let mark_commands = self.mark_commands(visible_bytes.clone());

        for i in visible_bytes.step_by(self.bytes_per_line) {
            let normalized_i = i - start_index;
            let normalized_end = std::cmp::min(max_bytes, normalized_i + self.bytes_per_line);
            self.draw_row(
                stdout,
                &visible_bytes_cow[normalized_i..normalized_end],
                i,
                &mark_commands[normalized_i..normalized_end],
            )?;
        }
        queue!(stdout, terminal::Clear(terminal::ClearType::UntilNewLine))?;

        let new_full_rows =
            (end_index - start_index + self.bytes_per_line - 1) / self.bytes_per_line;
        if new_full_rows != self.last_visible_rows.get() {
            queue!(stdout, terminal::Clear(terminal::ClearType::FromCursorDown))?;
            self.last_visible_rows.set(new_full_rows);
        }

        self.draw_statusline(stdout)?;

        Ok(begin.elapsed())
    }

    fn handle_event_default(&mut self, stdout: &mut impl Write, event: Event) -> Result<()> {
        match event {
            Event::Resize(x, y) => {
                self.size = (x, y);
                self.draw(stdout)?;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn scroll_down(&mut self, stdout: &mut impl Write, line_count: usize) -> Result<()> {
        self.start_offset += 0x10 * line_count;

        if line_count > (self.size.1 - 1) as usize {
            self.draw(stdout)?;
            Ok(())
        } else {
            queue!(
                stdout,
                terminal::ScrollUp(line_count as u16),
                // important: first scroll, then clear the line
                // I don't know why, but this prevents flashing on the statusline
                cursor::MoveTo(0, self.size.1 - 2),
                terminal::Clear(terminal::ClearType::CurrentLine),
            )?;
            let invalidated_rows =
                (self.size.1 - 1 - line_count as u16..=self.size.1 - 2).collect();
            self.draw_rows(stdout, &invalidated_rows) // -1 is statusline
        }
    }
    fn scroll_up(&mut self, stdout: &mut impl Write, line_count: usize) -> Result<()> {
        self.start_offset -= 0x10 * line_count;

        if line_count > (self.size.1 - 1) as usize {
            self.draw(stdout)?;
            Ok(())
        } else {
            queue!(
                stdout,
                terminal::ScrollDown(line_count as u16),
                cursor::MoveTo(0, self.size.1 - 1),
                terminal::Clear(terminal::ClearType::CurrentLine),
            )?;
            let invalidated_rows = (0..line_count as u16).collect();
            self.draw_rows(stdout, &invalidated_rows) // -1 is statusline
        }
    }

    fn maybe_update_offset(&mut self, stdout: &mut impl Write) -> Result<()> {
        if self.buffer.data.is_empty() {
            self.start_offset = 0;
            return Ok(());
        }

        let main_cursor_offset = self.buffer.selection.main_cursor_offset();
        let visible_bytes = self.visible_bytes();
        let delta = if main_cursor_offset < visible_bytes.start {
            main_cursor_offset as isize - visible_bytes.start as isize
        } else if main_cursor_offset >= visible_bytes.end {
            main_cursor_offset as isize - (visible_bytes.end as isize - 1)
        } else {
            return Ok(());
        };
        if delta < 0 {
            let line_delta =
                (delta - self.bytes_per_line as isize + 1) / self.bytes_per_line as isize;
            self.scroll_up(stdout, line_delta.abs() as usize)
        } else {
            let line_delta =
                (delta + self.bytes_per_line as isize - 1) / self.bytes_per_line as isize;
            self.scroll_down(stdout, line_delta as usize)
        }
    }

    fn maybe_update_offset_and_draw(&mut self, stdout: &mut impl Write) -> Result<()> {
        if self.buffer.data.is_empty() {
            self.start_offset = 0;
            self.draw_empty(stdout)?;
            return Ok(());
        }

        let main_cursor_offset = dbg!(self.buffer.selection.main_cursor_offset());
        let visible_bytes = self.visible_bytes();
        if main_cursor_offset < visible_bytes.start {
            self.start_offset = main_cursor_offset - main_cursor_offset % self.bytes_per_line;
        } else if main_cursor_offset >= visible_bytes.end {
            let bytes_per_screen = self.size.1 as usize * self.bytes_per_line;
            self.start_offset = (main_cursor_offset - main_cursor_offset % self.bytes_per_line
                + self.bytes_per_line)
                .saturating_sub(bytes_per_screen);
        }

        self.draw(stdout)?;
        Ok(())
    }

    fn transition_dirty_bytes(
        &mut self,
        stdout: &mut impl Write,
        dirty_bytes: DirtyBytes,
    ) -> Result<()> {
        match dirty_bytes {
            DirtyBytes::ChangeInPlace(intervals) => {
                self.maybe_update_offset(stdout)?;

                let visible: Interval = self.visible_bytes().into();
                let invalidated_rows = intervals
                    .into_iter()
                    .flat_map(|x| {
                        let intersection = visible.intersect(x);
                        if intersection.is_empty() {
                            0..0
                        } else {
                            intersection.start..intersection.end
                        }
                    })
                    .map(|byte| ((byte - self.start_offset) / self.bytes_per_line) as u16)
                    .collect();

                self.draw_rows(stdout, &invalidated_rows)
            }
            DirtyBytes::ChangeLength => self.maybe_update_offset_and_draw(stdout),
        }
    }

    fn transition(&mut self, stdout: &mut impl Write, transition: ModeTransition) -> Result<()> {
        match transition {
            ModeTransition::None => Ok(()),
            ModeTransition::DirtyBytes(dirty_bytes) => {
                self.transition_dirty_bytes(stdout, dirty_bytes)
            }
            ModeTransition::NewMode(mode) => {
                self.mode = mode;
                Ok(())
            }
            ModeTransition::ModeAndDirtyBytes(mode, dirty_bytes) => {
                self.mode = mode;
                self.transition_dirty_bytes(stdout, dirty_bytes)
            }
        }
    }

    pub fn run_event_loop(mut self, stdout: &mut impl Write) -> Result<()> {
        execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;

        self.last_draw_time = self.draw(stdout)?;
        terminal::enable_raw_mode()?;
        stdout.flush()?;

        loop {
            if !self.mode.takes_input() {
                break;
            }
            let evt = event::read()?;
            let transition = self
                .mode
                .transition(&evt, &mut self.buffer, self.bytes_per_line);
            if let Some(transition) = transition {
                self.transition(stdout, transition)?;
            } else {
                self.handle_event_default(stdout, evt)?;
            }

            self.draw_statusline(stdout)?;
            stdout.flush()?;
        }
        execute!(stdout, cursor::Show, terminal::LeaveAlternateScreen)?;
        terminal::disable_raw_mode()?;
        Ok(())
    }
}
