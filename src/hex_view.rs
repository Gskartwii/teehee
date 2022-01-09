use std::cell::Cell;
use std::cmp;
use std::collections::BTreeSet;
use std::fmt;
use std::ops::Range;
use std::time;

use crossterm::{
    cursor,
    event::{self, Event},
    execute, queue, style,
    style::{Color, Stylize},
    terminal, QueueableCommand, Result,
};
use xi_rope::Interval;

use super::buffer::*;
use super::mode::*;
use super::modes;
use crate::byte_properties::BytePropertiesFormatter;
use std::io::Write;
use crossterm::style::Attributes;

const VERTICAL: &str = "│";
const LEFTARROW: &str = "";

const COLOR_NULL: Color = Color::AnsiValue(150);
// const COLOR_OFFSET: Color = Color::AnsiValue(242);
const COLOR_ASCII_PRINTABLE: Color = Color::Cyan;
const COLOR_ASCII_WHITESPACE: Color = Color::Green;
const COLOR_ASCII_OTHER: Color = Color::Rgb {
    r: 232,
    g: 52,
    b: 210,
};
const COLOR_NONASCII: Color = Color::Yellow;

fn get_byte_color(byte: u8) -> Color {
    if byte == 0x00 {
        COLOR_NULL
    } else if byte.is_ascii_graphic() {
        COLOR_ASCII_PRINTABLE
    } else if byte.is_ascii_whitespace() {
        COLOR_ASCII_WHITESPACE
    } else if byte.is_ascii() {
        COLOR_ASCII_OTHER
    } else {
        COLOR_NONASCII
    }
}

fn colorize_byte(byte: u8, style_cmd: StylingCommand) -> StylingCommand {
    let default_content_style = style::ContentStyle {
        foreground_color: None,
        background_color: None,
        attributes: Default::default(),
    };

    let start_style = *style_cmd.start_style().unwrap_or(&default_content_style);

    style_cmd.with_start_style(PrioritizedStyle {
        style: style::ContentStyle {
            foreground_color: Some(get_byte_color(byte)),
            background_color: start_style.background_color,
            attributes: start_style.attributes,
        },
        priority: Priority::Basic,
    })
}

#[derive(Debug, Clone, Copy)]
enum Priority {
    Basic,
    #[allow(dead_code)]
    Mark,
    Selection,
    Cursor,
}

#[derive(Debug, Clone)]
struct PrioritizedStyle {
    style: style::ContentStyle,
    #[allow(dead_code)]
    priority: Priority,
}

#[derive(Debug, Clone)]
struct StylingCommand {
    start: Option<PrioritizedStyle>,
    mid: Option<PrioritizedStyle>,
    end: Option<PrioritizedStyle>,
}

impl Default for StylingCommand {
    fn default() -> Self {
        Self {
            start: None,
            mid: None,
            end: Some(PrioritizedStyle {
                style: style::ContentStyle {
                    foreground_color: Some(Color::White),
                    background_color: Some(Color::Reset),
                    attributes: Attributes::default(),
                },
                priority: Priority::Basic,
            })
        }
    }
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
            write!(f, "<{:02x}>", self.0)
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

macro_rules! d_queue {
    ($writer:expr $(, $command:expr)* $(,)?) => {{
        Ok::<_, crossterm::ErrorKind>(&mut *$writer)
            $(.and_then(|mut writer| {
                QueueableCommand::queue(&mut writer, $command)?;
                Ok(writer)
            }))*
            .map(|_| ())
    }}
}

impl StatusLinePrompter for modes::search::Search {
    fn render_with_size(
        &self,
        stdout: &mut dyn Write,
        mut max_width: usize,
        last_start_col: usize,
    ) -> Result<usize> {
        let mut start_column = last_start_col;
        d_queue!(
            stdout,
            style::PrintStyledContent(
                style::style("search:")
                    .with(style::Color::White)
                    .on(style::Color::Blue),
            )
        )?;
        max_width -= "search:".len();

        // Make sure start_column is between self.cursor and the length of the pattern
        if self.pattern.pieces.len() <= start_column {
            start_column = std::cmp::max(1, self.pattern.pieces.len()) - 1;
        } else if self.cursor < start_column {
            start_column = self.cursor;
        }

        if self.hex {
            if self.cursor >= start_column + max_width / 3 {
                start_column = self.cursor - max_width / 3 + 1;
            }
            let last_byte = std::cmp::min(self.pattern.pieces.len(), start_column + max_width / 3);

            let normalized_cursor = self.cursor - start_column;
            for (i, piece) in self.pattern.pieces[start_column..last_byte]
                .iter()
                .enumerate()
            {
                match piece {
                    PatternPiece::Literal(byte) if normalized_cursor != i => {
                        d_queue!(stdout, style::Print(format!("{:02x} ", byte)))?
                    }
                    PatternPiece::Literal(byte)
                        if normalized_cursor == i && self.hex_half.is_some() =>
                    {
                        d_queue!(
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
                    PatternPiece::Literal(byte) => d_queue!(
                        stdout,
                        style::PrintStyledContent(
                            style::style(format!("{:02x}", byte))
                                .with(style::Color::Black)
                                .on(style::Color::White)
                        ),
                        style::Print(" "),
                    )?,
                    PatternPiece::Wildcard if normalized_cursor != i => d_queue!(
                        stdout,
                        style::PrintStyledContent(style::style("** ").with(style::Color::DarkRed))
                    )?,
                    PatternPiece::Wildcard => d_queue!(
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
                d_queue!(
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

        max_width -= (self.cursor == self.pattern.pieces.len()) as usize;

        use modes::search::PatternPiece;
        let mut lengths = self.pattern.pieces[start_column..]
            .iter()
            .map(|x| match x {
                PatternPiece::Wildcard => 1,
                PatternPiece::Literal(0x20) => 1,
                PatternPiece::Literal(byte) if byte.is_ascii_graphic() => 1,
                PatternPiece::Literal(_) => 4,
            })
            .collect::<Vec<_>>();
        let required_length: usize = lengths[..self.cursor - start_column].iter().sum();
        if required_length > max_width {
            let mut remaining_delta = (required_length - max_width) as isize;
            let num_dropped_pieces = lengths
                .iter()
                .position(|&x| {
                    let is_done = remaining_delta <= 0;
                    remaining_delta -= x as isize;
                    is_done
                })
                .unwrap();
            start_column += num_dropped_pieces;
            lengths.drain(..num_dropped_pieces);
        }

        let normalized_cursor = self.cursor - start_column;
        for ((i, piece), length) in self.pattern.pieces[start_column..]
            .iter()
            .enumerate()
            .zip(lengths)
        {
            if max_width < length {
                break;
            }
            max_width -= length;
            match piece {
                PatternPiece::Literal(byte)
                    if normalized_cursor != i && (byte.is_ascii_graphic() || *byte == 0x20) =>
                {
                    d_queue!(stdout, style::Print(format!("{}", *byte as char)))?
                }
                PatternPiece::Literal(byte) if normalized_cursor != i => d_queue!(
                    stdout,
                    style::PrintStyledContent(
                        style::style(format!("<{:02x}>", byte))
                            .with(style::Color::Black)
                            .on(style::Color::DarkGrey)
                    ),
                )?,
                PatternPiece::Literal(byte)
                    if normalized_cursor == i && (byte.is_ascii_graphic() || *byte == 0x20) =>
                {
                    d_queue!(
                        stdout,
                        style::PrintStyledContent(
                            style::style(format!("{}", *byte as char))
                                .with(style::Color::Black)
                                .on(style::Color::White)
                        ),
                    )?
                }
                PatternPiece::Literal(byte) => d_queue!(
                    stdout,
                    style::PrintStyledContent(
                        style::style(format!("<{:02x}>", byte))
                            .with(style::Color::Black)
                            .on(style::Color::White)
                    ),
                )?,
                PatternPiece::Wildcard if normalized_cursor != i => d_queue!(
                    stdout,
                    style::PrintStyledContent(style::style("*").with(style::Color::DarkRed))
                )?,
                PatternPiece::Wildcard => d_queue!(
                    stdout,
                    style::PrintStyledContent(
                        style::style("*")
                            .with(style::Color::DarkRed)
                            .on(style::Color::White)
                    ),
                )?,
            }
        }

        if self.cursor == self.pattern.pieces.len() {
            d_queue!(
                stdout,
                style::PrintStyledContent(
                    style::style(" ")
                        .with(style::Color::Black)
                        .on(style::Color::White)
                ),
            )?;
        }

        Ok(start_column)
    }
}

impl StatusLinePrompter for modes::command::Command {
    fn render_with_size(
        &self,
        stdout: &mut dyn Write,
        mut max_width: usize,
        last_start_col: usize,
    ) -> Result<usize> {
        let mut start_column = last_start_col;
        d_queue!(
            stdout,
            style::PrintStyledContent(
                style::style(":")
                    .with(style::Color::White)
                    .on(style::Color::Blue),
            )
        )?;
        max_width -= 1;

        // Make sure start_column is between self.cursor and the length of the pattern
        if self.command.len() <= start_column {
            start_column = std::cmp::max(1, self.command.len()) - 1;
        } else if self.cursor < start_column {
            start_column = self.cursor;
        }

        max_width -= (self.cursor == self.command.len()) as usize;

        let required_length = self.cursor - start_column;
        if required_length > max_width {
            start_column += required_length - max_width;
        }

        d_queue!(
            stdout,
            style::Print(
                &self.command
                    [start_column..std::cmp::min(self.command.len(), start_column + max_width)]
            )
        )?;

        if self.cursor == self.command.len() {
            d_queue!(
                stdout,
                style::PrintStyledContent(
                    style::style(" ")
                        .with(style::Color::Black)
                        .on(style::Color::White)
                ),
            )?;
        }

        Ok(start_column)
    }
}

pub struct HexView {
    buffers: Buffers,
    size: (u16, u16),
    bytes_per_line: usize,
    start_offset: usize,
    last_visible_rows: Cell<usize>,
    last_visible_prompt_col: Cell<usize>,
    last_draw_time: time::Duration,

    mode: Box<dyn Mode>,
    info: Option<String>,
}

impl HexView {
    pub fn with_buffers(buffers: Buffers) -> HexView {
        HexView {
            buffers,
            bytes_per_line: 0x10,
            start_offset: 0,
            size: terminal::size().unwrap(),
            last_visible_rows: Cell::new(0),
            last_visible_prompt_col: Cell::new(0),

            last_draw_time: Default::default(),

            mode: Box::new(modes::normal::Normal::new()),
            info: None,
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
            let style_cmd = colorize_byte(byte, style_cmd);
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
            let style_cmd = colorize_byte(byte, style_cmd);
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
        end_style: Option<StylingCommand>,
        byte_properties_line: Option<String>,
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

        let mut padding_length = if bytes.is_empty() {
            self.bytes_per_line * 3
        } else {
            (self.bytes_per_line - bytes.len()) % self.bytes_per_line * 3
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

        let padding_length = if bytes.is_empty() {
            self.bytes_per_line
        } else {
            (self.bytes_per_line - bytes.len()) % self.bytes_per_line
        } + 1;

        queue!(stdout, style::Print(make_padding(padding_length)))?;
        self.draw_separator(stdout)?;

        if let Some(byte_properties_line) = byte_properties_line {
            queue!(stdout, style::Print(byte_properties_line))?;
        }

        queue!(stdout, terminal::Clear(terminal::ClearType::UntilNewLine))?;

        Ok(())
    }

    fn visible_bytes(&self) -> Range<usize> {
        self.start_offset
            ..cmp::min(
                self.buffers.current().data.len() + 1,
                self.start_offset + (self.size.1 - 1) as usize * self.bytes_per_line,
            )
    }

    fn default_style(&self) -> PrioritizedStyle {
        PrioritizedStyle {
            style: style::ContentStyle::new()
                .with(style::Color::White)
                .on(style::Color::Reset),
            priority: Priority::Basic,
        }
    }

    fn active_selection_style(&self) -> PrioritizedStyle {
        PrioritizedStyle {
            style: style::ContentStyle::new()
                .with(style::Color::Black)
                .on(style::Color::DarkYellow),
            priority: Priority::Selection,
        }
    }

    fn inactive_selection_style(&self) -> PrioritizedStyle {
        PrioritizedStyle {
            style: style::ContentStyle::new()
                .with(style::Color::Black)
                .on(style::Color::DarkGrey),
            priority: Priority::Selection,
        }
    }

    fn active_caret_style(&self) -> PrioritizedStyle {
        PrioritizedStyle {
            style: style::ContentStyle::new()
                .with(style::Color::AnsiValue(16))
                .on(style::Color::Rgb {
                    r: 107,
                    g: 108,
                    b: 128,
                }),
            priority: Priority::Cursor,
        }
    }

    fn inactive_caret_style(&self) -> PrioritizedStyle {
        PrioritizedStyle {
            style: style::ContentStyle::new()
                .with(style::Color::Black)
                .on(style::Color::DarkGrey),
            priority: Priority::Cursor,
        }
    }

    fn empty_caret_style(&self) -> PrioritizedStyle {
        PrioritizedStyle {
            style: style::ContentStyle::new().on(style::Color::Green),
            priority: Priority::Cursor,
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
                    if self.mode.has_half_cursor() {
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
        let buf = self.buffers.current();
        let mut length = 0;
        length += 1; // leftarrow
        length += 2 + buf.name().len();
        if buf.dirty {
            length += 3;
        }
        length += 1; // leftarrow
        length += 2 + self.mode.name().len();
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
        if let Some(info) = &self.info {
            queue!(
                stdout,
                cursor::MoveTo(0, self.size.1 - 1),
                terminal::Clear(terminal::ClearType::CurrentLine),
                style::PrintStyledContent(
                    style::style(info)
                        .with(style::Color::White)
                        .on(style::Color::Blue)
                ),
                cursor::MoveTo(self.size.0 - line_length as u16, self.size.1),
            )?;
        } else {
            queue!(
                stdout,
                cursor::MoveTo(self.size.0 - line_length as u16, self.size.1),
                terminal::Clear(terminal::ClearType::CurrentLine),
            )?;
        }

        self.draw_statusline_here(stdout)?;

        let any_mode = self.mode.as_any();
        let prompter = if let Some(statusliner) = any_mode.downcast_ref::<modes::search::Search>() {
            Some(statusliner as &dyn StatusLinePrompter)
        } else {
            any_mode
                .downcast_ref::<modes::command::Command>()
                .map(|statusliner| statusliner as &dyn StatusLinePrompter)
        };

        if let Some(statusliner) = prompter {
            queue!(stdout, cursor::MoveTo(0, self.size.1))?;
            let prev_col = self.last_visible_prompt_col.get();
            let new_col = statusliner.render_with_size(stdout, self.size.0 as usize, prev_col)?;
            self.last_visible_prompt_col.set(new_col);
        }

        Ok(())
    }

    fn overflow_cursor_style(&self) -> Option<StylingCommand> {
        self.buffers.current().overflow_sel_style().map(|style| {
            match style {
                OverflowSelectionStyle::CursorTail | OverflowSelectionStyle::Cursor
                    if self.mode.has_half_cursor() =>
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

    fn draw_rows(&self, stdout: &mut impl Write, invalidated_rows: &BTreeSet<u16>) -> Result<()> {
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

        let current_bytes = self
            .buffers
            .current()
            .selection
            .regions_in_range(visible_bytes.start, visible_bytes.end)
            .iter()
            .filter(|region| region.is_main())
            .next()
            .map(|v| {
                let start = v.caret - start_index;
                let end = if start + 4 > visible_bytes_cow.len() {
                    visible_bytes_cow.len()
                } else {
                    start + 4
                };
                &visible_bytes_cow[start..end]
            })
            .unwrap_or_else(|| &[0]);

        let mut byte_properties = BytePropertiesFormatter::new(current_bytes).iter();

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
                if i + self.bytes_per_line > self.buffers.current().data.len() {
                    self.overflow_cursor_style()
                } else {
                    None
                },
                byte_properties.next(),
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

        let current_bytes = self
            .buffers
            .current()
            .selection
            .regions_in_range(visible_bytes.start, visible_bytes.end)
            .iter()
            .filter(|region| region.is_main())
            .next()
            .map(|v| {
                let start = v.caret - start_index;
                let end = if start + 4 > visible_bytes_cow.len() {
                    visible_bytes_cow.len()
                } else {
                    start + 4
                };
                &visible_bytes_cow[start..end]
            })
            .unwrap_or_else(|| &[]);

        let mut byte_properties = BytePropertiesFormatter::new(current_bytes).iter();

        for i in visible_bytes.step_by(self.bytes_per_line) {
            let normalized_i = i - start_index;
            let normalized_end = std::cmp::min(max_bytes, normalized_i + self.bytes_per_line);
            self.draw_row(
                stdout,
                &visible_bytes_cow[normalized_i..normalized_end],
                i,
                &mark_commands[normalized_i..normalized_end],
                if i + self.bytes_per_line > self.buffers.current().data.len() {
                    self.overflow_cursor_style()
                } else {
                    None
                },
                byte_properties.next(),
            )?;
        }

        let new_full_rows =
            (end_index - start_index + self.bytes_per_line - 1) / self.bytes_per_line;
        if new_full_rows != self.last_visible_rows.get() {
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

            let mut invalidated_rows: BTreeSet<u16> =
                (self.size.1 - 1 - line_count as u16..=self.size.1 - 2).collect();
            invalidated_rows.extend(0..BytePropertiesFormatter::height());
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

            let mut invalidated_rows: BTreeSet<u16> = (0..line_count as u16).collect();
            invalidated_rows.extend(0..BytePropertiesFormatter::height());
            self.draw_rows(stdout, &invalidated_rows) // -1 is statusline
        }
    }

    fn maybe_update_offset(&mut self, stdout: &mut impl Write) -> Result<()> {
        if self.buffers.current().data.is_empty() {
            self.start_offset = 0;
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
                (delta - self.bytes_per_line as isize + 1) / self.bytes_per_line as isize;
            self.scroll_up(stdout, line_delta.abs() as usize)
        } else {
            let line_delta =
                (delta + self.bytes_per_line as isize - 1) / self.bytes_per_line as isize;
            self.scroll_down(stdout, line_delta as usize)
        }
    }

    fn maybe_update_offset_and_draw(&mut self, stdout: &mut impl Write) -> Result<()> {
        let main_cursor_offset = self.buffers.current().selection.main_cursor_offset();
        let visible_bytes = self.visible_bytes();
        if main_cursor_offset < visible_bytes.start {
            self.start_offset = main_cursor_offset - main_cursor_offset % self.bytes_per_line;
        } else if main_cursor_offset >= visible_bytes.end {
            let bytes_per_screen = (self.size.1 as usize - 1) * self.bytes_per_line; // -1 for statusline
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
                let mut invalidated_rows: BTreeSet<u16> = intervals
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

                invalidated_rows.extend(0..BytePropertiesFormatter::height());
                self.draw_rows(stdout, &invalidated_rows)
            }
            DirtyBytes::ChangeLength => self.maybe_update_offset_and_draw(stdout),
        }
    }

    fn transition(&mut self, stdout: &mut impl Write, transition: ModeTransition) -> Result<()> {
        self.info = None;
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
            ModeTransition::ModeAndInfo(mode, info) => {
                self.mode = mode;
                self.info = Some(info);
                Ok(())
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
                .transition(&evt, &mut self.buffers, self.bytes_per_line);
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
