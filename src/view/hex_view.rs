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

use super::prompt::*;
use super::style::*;
use super::view_options::{DirtyBytes, ViewOptions};
use crate::buffer::*;
use crate::mode::*;
use crate::modes;
use std::io::Write;

const VERTICAL: &str = "│";
const LEFTARROW: &str = "";

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

pub struct HexView {
    buffers: Buffers,
    options: ViewOptions,
    last_visible_rows: Cell<usize>,
    last_visible_prompt_col: Cell<usize>,
    last_draw_time: time::Duration,

    mode_stack: Vec<Box<dyn Mode>>,
}

impl HexView {
    pub fn with_buffers(buffers: Buffers) -> HexView {
        HexView {
            buffers,
            options: ViewOptions::new(),
            last_visible_rows: Cell::new(0),
            last_visible_prompt_col: Cell::new(0),

            last_draw_time: Default::default(),

            mode_stack: vec![Box::new(modes::normal::Normal::new())],
        }
    }

    fn mode(&self) -> &dyn Mode {
        &(**self.mode_stack.last().unwrap())
    }

    fn reset_normal_mode(&mut self) {
        self.mode_stack = vec![Box::new(modes::normal::Normal::new())];
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
        if offset < self.options.start_offset {
            return None;
        }
        let normalized_offset = offset - self.options.start_offset;
        let bytes_per_line = self.options.bytes_per_line;
        let max_bytes = bytes_per_line * self.options.size.1 as usize;
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
        end_style: Option<StylingCommand>,
    ) -> Result<()> {
        let row_num = self.offset_to_row(offset).unwrap();

        queue!(stdout, cursor::MoveTo(0, row_num))?;
        self.draw_hex_row(
            stdout,
            bytes.iter().copied().zip(mark_commands.iter().cloned()),
        )?;

        let mut padding_length = if bytes.len() == 0 {
            self.options.bytes_per_line * 3
        } else {
            (self.options.bytes_per_line - bytes.len()) % self.options.bytes_per_line * 3
        };
        if let Some(style_cmd) = &end_style {
            padding_length -= 2;

            if let Some(start_cmd) = style_cmd.start_style() {
                queue_style(stdout, start_cmd)?;
            }
            queue!(stdout, style::Print(" "))?;
            if let Some(mid_cmd) = style_cmd.mid_style() {
                queue_style(stdout, mid_cmd)?;
            }
            queue!(stdout, style::Print(" "))?;
            if let Some(end_cmd) = style_cmd.end_style() {
                queue_style(stdout, end_cmd)?;
            }
        }

        queue!(stdout, style::Print(make_padding(padding_length)))?;
        self.draw_separator(stdout)?;
        self.draw_ascii_row(
            stdout,
            bytes.iter().copied().zip(mark_commands.iter().cloned()),
        )?;
        if let Some(style_cmd) = end_style {
            if let Some(start_cmd) = style_cmd.start_style() {
                queue_style(stdout, start_cmd)?;
            }
            queue!(stdout, style::Print(" "))?;
            if let Some(end_cmd) = style_cmd.end_style() {
                queue_style(stdout, end_cmd)?;
            }
        }
        queue!(stdout, terminal::Clear(terminal::ClearType::UntilNewLine))?;

        Ok(())
    }

    fn visible_bytes(&self) -> Range<usize> {
        self.options.start_offset
            ..cmp::min(
                self.buffers.current().data.len() + 1,
                self.options.start_offset + (self.options.size.1 - 1) as usize * self.options.bytes_per_line,
            )
    }

    fn default_style(&self) -> PrioritizedStyle {
        PrioritizedStyle {
            style: style::ContentStyle::new()
                .foreground(style::Color::White)
                .background(style::Color::Black),
        }
    }
    fn active_selection_style(&self) -> PrioritizedStyle {
        PrioritizedStyle {
            style: style::ContentStyle::new()
                .foreground(style::Color::Black)
                .background(style::Color::DarkYellow),
        }
    }
    fn inactive_selection_style(&self) -> PrioritizedStyle {
        PrioritizedStyle {
            style: style::ContentStyle::new()
                .foreground(style::Color::Black)
                .background(style::Color::DarkGrey),
        }
    }
    fn active_caret_style(&self) -> PrioritizedStyle {
        PrioritizedStyle {
            style: style::ContentStyle::new()
                .foreground(style::Color::AnsiValue(16))
                .background(style::Color::White),
        }
    }
    fn inactive_caret_style(&self) -> PrioritizedStyle {
        PrioritizedStyle {
            style: style::ContentStyle::new()
                .foreground(style::Color::Black)
                .background(style::Color::DarkGrey),
        }
    }
    fn empty_caret_style(&self) -> PrioritizedStyle {
        PrioritizedStyle {
            style: style::ContentStyle::new().background(style::Color::Green),
        }
    }

    fn mark_commands(&self, visible: Range<usize>) -> Vec<StylingCommand> {
        let mut mark_commands = vec![StylingCommand::default(); visible.len()];
        let mut selected_regions = self
            .buffers
            .current()
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
                    let caret_style = if selected_regions[0].is_main() {
                        self.active_caret_style()
                    } else {
                        self.inactive_caret_style()
                    };
                    if self.mode().has_half_cursor() {
                        if i == selected_regions[0].min() {
                            caret_cmd = caret_cmd
                                .with_mid_style(caret_style)
                                .with_end_style(base_style);
                        } else {
                            caret_cmd = caret_cmd
                                .with_start_style(base_style)
                                .with_mid_style(caret_style);
                        }
                    } else {
                        caret_cmd = caret_cmd
                            .with_start_style(caret_style)
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

            if i % self.options.bytes_per_line == 0 && mark_commands[normalized].start_style().is_none() {
                // line starts: restore applied style
                mark_commands[normalized] = mark_commands[normalized]
                    .clone()
                    .with_start_style(command_stack.last().unwrap().clone());
            } else if (i + 1) % self.options.bytes_per_line == 0 {
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
        let buf = self.buffers.current();
        let mut length = 0;
        length += 1; // leftarrow
        length += 2 + buf.name().len();
        if buf.dirty {
            length += 3;
        }
        length += 1; // leftarrow
        length += 2 + self.mode().name().len();
        length += 1; // leftarrow
        length += format!(
            " {} sels ({}) ",
            buf.selection.len(),
            buf.selection.main_selection + 1
        )
        .len();
        length += 1; // leftarrow
        if !buf.data.is_empty() {
            length += format!(
                " {:x}/{:x} ",
                buf.selection.main_cursor_offset(),
                buf.data.len() - 1
            )
            .len();
        } else {
            length += " empty ".len();
        }
        length
    }

    fn draw_statusline_here(&self, stdout: &mut impl Write) -> Result<()> {
        let buf = self.buffers.current();
        queue!(
            stdout,
            style::PrintStyledContent(style::style(LEFTARROW).with(Color::Red)),
            style::PrintStyledContent(
                style::style(format!(
                    " {}{} ",
                    self.buffers.current().name(),
                    if self.buffers.current().dirty {
                        "[+]"
                    } else {
                        ""
                    }
                ))
                .with(Color::White)
                .on(Color::Red)
            ),
            style::PrintStyledContent(
                style::style(LEFTARROW)
                    .with(Color::DarkYellow)
                    .on(Color::Red)
            ),
            style::PrintStyledContent(
                style::style(format!(" {} ", self.mode().name()))
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
                    buf.selection.len(),
                    buf.selection.main_selection + 1
                ))
                .with(Color::AnsiValue(16))
                .on(Color::White)
            ),
        )?;
        if !buf.data.is_empty() {
            queue!(
                stdout,
                style::PrintStyledContent(
                    style::style(LEFTARROW).with(Color::Blue).on(Color::White)
                ),
                style::PrintStyledContent(
                    style::style(format!(
                        " {:x}/{:x} ",
                        buf.selection.main_cursor_offset(),
                        buf.data.len() - 1,
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
        if let Some(info) = &self.options.info {
            queue!(
                stdout,
                cursor::MoveTo(0, self.options.size.1 - 1),
                terminal::Clear(terminal::ClearType::CurrentLine),
                style::PrintStyledContent(
                    style::style(info)
                        .with(style::Color::White)
                        .on(style::Color::Blue)
                ),
                cursor::MoveTo(self.options.size.0 - line_length as u16, self.options.size.1),
            )?;
        } else {
            queue!(
                stdout,
                cursor::MoveTo(self.options.size.0 - line_length as u16, self.options.size.1),
                terminal::Clear(terminal::ClearType::CurrentLine),
            )?;
        }

        self.draw_statusline_here(stdout)?;

        let any_mode = self.mode().as_any();
        let prompter = if let Some(statusliner) = any_mode.downcast_ref::<modes::search::Search>() {
            Some(statusliner as &dyn StatusLinePrompter)
        } else if let Some(statusliner) = any_mode.downcast_ref::<modes::command::Command>() {
            Some(statusliner as &dyn StatusLinePrompter)
        } else {
            None
        };

        if let Some(statusliner) = prompter {
            queue!(stdout, cursor::MoveTo(0, self.options.size.1))?;
            let prev_col = self.last_visible_prompt_col.get();
            let new_col = statusliner.render_with_size(stdout, self.options.size.0 as usize, prev_col)?;
            self.last_visible_prompt_col.set(new_col);
        }

        Ok(())
    }

    fn overflow_cursor_style(&self) -> Option<StylingCommand> {
        self.buffers.current().overflow_sel_style().map(|style| {
            match style {
                OverflowSelectionStyle::CursorTail | OverflowSelectionStyle::Cursor
                    if self.mode().has_half_cursor() =>
                {
                    StylingCommand::default().with_mid_style(self.empty_caret_style())
                }
                OverflowSelectionStyle::CursorTail | OverflowSelectionStyle::Cursor => {
                    StylingCommand::default().with_start_style(self.empty_caret_style())
                }
                OverflowSelectionStyle::Tail => StylingCommand::default(),
            }
            .with_end_style(self.default_style())
        })
    }

    fn draw_rows(&self, stdout: &mut impl Write, invalidated_rows: &HashSet<u16>) -> Result<()> {
        let visible_bytes = self.visible_bytes();
        let start_index = visible_bytes.start;
        let end_index = visible_bytes.end;

        let visible_bytes_cow = self
            .buffers
            .current()
            .data
            .slice_to_cow(start_index..end_index);

        let max_bytes = visible_bytes_cow.len();
        let mark_commands = self.mark_commands(visible_bytes.clone());

        for i in visible_bytes.step_by(self.options.bytes_per_line) {
            if !invalidated_rows.contains(&self.offset_to_row(i).unwrap()) {
                continue;
            }

            let normalized_i = i - start_index;
            let normalized_end = std::cmp::min(max_bytes, normalized_i + self.options.bytes_per_line);
            self.draw_row(
                stdout,
                &visible_bytes_cow[normalized_i..normalized_end],
                i,
                &mark_commands[normalized_i..normalized_end],
                if i + self.options.bytes_per_line > self.buffers.current().data.len() {
                    self.overflow_cursor_style()
                } else {
                    None
                },
            )?;
        }

        Ok(())
    }

    fn draw(&self, stdout: &mut impl Write) -> Result<time::Duration> {
        let begin = time::Instant::now();

        queue!(
            stdout,
            cursor::MoveTo(0, 0),
            terminal::Clear(terminal::ClearType::All)
        )?;

        let visible_bytes = self.visible_bytes();
        let start_index = visible_bytes.start;
        let end_index = visible_bytes.end;
        let visible_bytes_cow = self
            .buffers
            .current()
            .data
            .slice_to_cow(start_index..end_index);

        let max_bytes = visible_bytes_cow.len();
        let mark_commands = self.mark_commands(visible_bytes.clone());

        for i in visible_bytes.step_by(self.options.bytes_per_line) {
            let normalized_i = i - start_index;
            let normalized_end = std::cmp::min(max_bytes, normalized_i + self.options.bytes_per_line);
            self.draw_row(
                stdout,
                &visible_bytes_cow[normalized_i..normalized_end],
                i,
                &mark_commands[normalized_i..normalized_end],
                if i + self.options.bytes_per_line > self.buffers.current().data.len() {
                    self.overflow_cursor_style()
                } else {
                    None
                },
            )?;
        }

        let new_full_rows =
            (end_index - start_index + self.options.bytes_per_line - 1) / self.options.bytes_per_line;
        if new_full_rows != self.last_visible_rows.get() {
            self.last_visible_rows.set(new_full_rows);
        }

        self.draw_statusline(stdout)?;

        Ok(begin.elapsed())
    }

    fn handle_event_default(&mut self, stdout: &mut impl Write, event: Event) -> Result<()> {
        match event {
            Event::Resize(x, y) => {
                self.options.size = (x, y);
                self.draw(stdout)?;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn scroll_down(&mut self, stdout: &mut impl Write, line_count: usize) -> Result<()> {
        self.options.start_offset += 0x10 * line_count;

        if line_count > (self.options.size.1 - 1) as usize {
            self.draw(stdout)?;
            Ok(())
        } else {
            queue!(
                stdout,
                terminal::ScrollUp(line_count as u16),
                // important: first scroll, then clear the line
                // I don't know why, but this prevents flashing on the statusline
                cursor::MoveTo(0, self.options.size.1 - 2),
                terminal::Clear(terminal::ClearType::CurrentLine),
            )?;
            let invalidated_rows =
                (self.options.size.1 - 1 - line_count as u16..=self.options.size.1 - 2).collect();
            self.draw_rows(stdout, &invalidated_rows) // -1 is statusline
        }
    }
    fn scroll_up(&mut self, stdout: &mut impl Write, line_count: usize) -> Result<()> {
        self.options.start_offset -= 0x10 * line_count;

        if line_count > (self.options.size.1 - 1) as usize {
            self.draw(stdout)?;
            Ok(())
        } else {
            queue!(
                stdout,
                terminal::ScrollDown(line_count as u16),
                cursor::MoveTo(0, self.options.size.1 - 1),
                terminal::Clear(terminal::ClearType::CurrentLine),
            )?;
            let invalidated_rows = (0..line_count as u16).collect();
            self.draw_rows(stdout, &invalidated_rows) // -1 is statusline
        }
    }

    fn maybe_update_offset(&mut self, stdout: &mut impl Write) -> Result<()> {
        if self.buffers.current().data.is_empty() {
            self.options.start_offset = 0;
            return Ok(());
        }

        let main_cursor_offset = self.buffers.current().selection.main_cursor_offset();
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
                (delta - self.options.bytes_per_line as isize + 1) / self.options.bytes_per_line as isize;
            self.scroll_up(stdout, line_delta.abs() as usize)
        } else {
            let line_delta =
                (delta + self.options.bytes_per_line as isize - 1) / self.options.bytes_per_line as isize;
            self.scroll_down(stdout, line_delta as usize)
        }
    }

    fn maybe_update_offset_and_draw(&mut self, stdout: &mut impl Write) -> Result<()> {
        let main_cursor_offset = self.buffers.current().selection.main_cursor_offset();
        let visible_bytes = self.visible_bytes();
        if main_cursor_offset < visible_bytes.start {
            self.options.start_offset = main_cursor_offset - main_cursor_offset % self.options.bytes_per_line;
        } else if main_cursor_offset >= visible_bytes.end {
            let bytes_per_screen = (self.options.size.1 as usize - 1) * self.options.bytes_per_line; // -1 for statusline
            self.options.start_offset = (main_cursor_offset - main_cursor_offset % self.options.bytes_per_line
                + self.options.bytes_per_line)
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
                    .map(|byte| ((byte - self.options.start_offset) / self.options.bytes_per_line) as u16)
                    .collect();

                self.draw_rows(stdout, &invalidated_rows)
            }
            DirtyBytes::ChangeLength => self.maybe_update_offset_and_draw(stdout),
        }
    }

    fn transition(&mut self, stdout: &mut impl Write) -> Result<()> {
        if let Some(dirty_bytes) = self.options.dirty.take() {
            self.transition_dirty_bytes(stdout, dirty_bytes)?;
        }
        Ok(())
    }

    pub fn run_event_loop(mut self, stdout: &mut impl Write) -> Result<()> {
        execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;

        self.last_draw_time = self.draw(stdout)?;
        terminal::enable_raw_mode()?;
        stdout.flush()?;

        loop {
            if !self.mode().takes_input() {
                break;
            }
            let evt = event::read()?;
            self.options.info = None;

            let old_mode = self.mode_stack.pop().unwrap();
            match old_mode.transition(&evt, &mut self.buffers, &mut self.options) {
                ModeTransition::NotHandled(old) => {
                    self.mode_stack.push(old);
                    self.handle_event_default(stdout, evt)?;
                },
                ModeTransition::Pop => {
                    if self.mode_stack.is_empty() {
                        self.reset_normal_mode();
                    }
                    self.transition(stdout)?;
                },
                ModeTransition::Push(mut new) => {
                    self.mode_stack.append(&mut new);
                    self.transition(stdout)?;
                },
            }

            self.draw_statusline(stdout)?;
            stdout.flush()?;
        }
        execute!(stdout, cursor::Show, terminal::LeaveAlternateScreen)?;
        terminal::disable_raw_mode()?;
        Ok(())
    }
}
