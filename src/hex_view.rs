use std::cell::Cell;
use std::cmp;
use std::collections::HashSet;
use std::fmt;
use std::ops::Range;
use std::time;

use crossterm::{
    cursor, event,
    event::{Event, KeyCode, KeyEvent, KeyModifiers},
    execute, queue, style,
    style::Color,
    terminal, Result,
};

use super::byte_rope::*;
use super::operations::*;
use super::selection::*;
use std::io::Write;

const VERTICAL: &str = "│";
const RIGHTARROW: &str = "";

#[derive(Debug, Clone, Copy)]
enum State {
    Quitting,
    Normal,
    JumpTo { extend: bool },
    Split,
    Insert { before: bool, hex: bool },
}

impl State {
    fn name(&self) -> &'static str {
        match self {
            State::Quitting => "QUIT",
            State::Normal => "NORMAL",
            State::JumpTo { extend: true } => "EXTEND",
            State::JumpTo { extend: false } => "JUMP",
            State::Split => "SPLIT",
            State::Insert {
                before: true,
                hex: true,
            } => "INSERT (hex)",
            State::Insert {
                before: true,
                hex: false,
            } => "INSERT (ascii)",
            State::Insert {
                before: false,
                hex: true,
            } => "APPEND (hex)",
            State::Insert {
                before: false,
                hex: false,
            } => "APPEND (ascii)",
        }
    }
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

fn key_direction(key_code: KeyCode) -> Option<Direction> {
    match key_code {
        KeyCode::Char('h') | KeyCode::Char('H') => Some(Direction::Left),
        KeyCode::Char('j') | KeyCode::Char('J') => Some(Direction::Down),
        KeyCode::Char('k') | KeyCode::Char('K') => Some(Direction::Up),
        KeyCode::Char('l') | KeyCode::Char('L') => Some(Direction::Right),
        _ => None,
    }
}

fn split_width(key_code: KeyCode) -> Option<usize> {
    match key_code {
        KeyCode::Char('b') => Some(1),
        KeyCode::Char('w') => Some(2),
        KeyCode::Char('d') => Some(4),
        KeyCode::Char('q') => Some(8),
        KeyCode::Char('o') => Some(16),
        _ => None,
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

pub struct HexView {
    data: Rope,
    size: (u16, u16),
    bytes_per_line: usize,
    start_offset: usize,
    selection: Selection,
    last_visible_rows: Cell<usize>,
    use_half_cursor: bool,

    last_draw_time: time::Duration,

    state: State,
}

impl HexView {
    pub fn from_data(data: Vec<u8>) -> HexView {
        HexView {
            data: data.into(),
            bytes_per_line: 0x10,
            start_offset: 0,
            size: terminal::size().unwrap(),
            selection: Selection::new(),
            last_visible_rows: Cell::new(0),
            use_half_cursor: false,

            last_draw_time: Default::default(),

            state: State::Normal,
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
            queue!(stdout, style::Print(format!(" ")))?;
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
                self.data.len(),
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
    fn empty_caret_style(&self) -> PrioritizedStyle {
        PrioritizedStyle {
            style: style::ContentStyle::new().background(style::Color::Green),
            priority: Priority::Cursor,
        }
    }

    fn mark_commands(&self, visible: Range<usize>) -> Vec<StylingCommand> {
        let mut mark_commands = vec![StylingCommand::default(); visible.len()];
        let mut selected_regions = self.selection.regions_in_range(visible.start, visible.end);
        let mut command_stack = vec![self.default_style()];
        let start = visible.start;

        // Add to command stack those commands that being out of bounds
        if !selected_regions.is_empty() && selected_regions[0].min() < start {
            command_stack.push(self.selection_style());
        }

        for i in visible {
            let normalized = i - start;
            if !selected_regions.is_empty() {
                if selected_regions[0].min() == i {
                    command_stack.push(self.selection_style());
                    mark_commands[normalized] = mark_commands[normalized]
                        .clone()
                        .with_start_style(command_stack.last().unwrap().clone());
                }
                if selected_regions[0].caret == i {
                    let base_style = command_stack.last().unwrap().clone();
                    let mut caret_cmd = mark_commands[normalized].clone();
                    if self.use_half_cursor {
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

    fn draw_statusline(&self, stdout: &mut impl Write) -> Result<()> {
        queue!(
            stdout,
            cursor::MoveTo(0, self.size.1),
            terminal::Clear(terminal::ClearType::CurrentLine),
            style::PrintStyledContent(
                style::style(format!(" {} ", self.state.name()))
                    .with(Color::AnsiValue(16))
                    .on(Color::DarkYellow)
            ),
            style::PrintStyledContent(
                style::style(RIGHTARROW)
                    .with(Color::DarkYellow)
                    .on(Color::White)
            ),
            style::PrintStyledContent(
                style::style(format!(
                    " {} sels ({}) ",
                    self.selection.len(),
                    self.selection.main_selection + 1
                ))
                .with(Color::AnsiValue(16))
                .on(Color::White)
            ),
            style::PrintStyledContent(style::style(RIGHTARROW).with(Color::White).on(Color::Blue)),
        )?;
        if !self.data.is_empty() {
            queue!(
                stdout,
                style::PrintStyledContent(
                    style::style(format!(
                        " {:x}/{:x} ",
                        self.selection.main_cursor_offset(),
                        self.data.len() - 1,
                    ))
                    .with(Color::White)
                    .on(Color::Blue),
                ),
                style::PrintStyledContent(style::style(RIGHTARROW).with(Color::Blue))
            )?;
        } else {
            queue!(
                stdout,
                style::PrintStyledContent(
                    style::style(" empty ").with(Color::White).on(Color::Blue),
                ),
                style::PrintStyledContent(style::style(RIGHTARROW).with(Color::Blue))
            )?;
        }
        queue!(
            stdout,
            style::Print(format!(
                " -- debug: draw time {} ms",
                self.last_draw_time.as_millis()
            )),
        )?;
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
        if self.data.is_empty() {
            self.draw_empty(stdout)?;
            return Ok(());
        }
        let visible_bytes = self.visible_bytes();
        let start_index = visible_bytes.start;
        let end_index = visible_bytes.end;

        let visible_bytes_cow = self.data.slice_to_cow(start_index..end_index);

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
        if self.data.is_empty() {
            self.draw_empty(stdout)?;
            return Ok(begin.elapsed());
        }

        queue!(stdout, cursor::MoveTo(0, 0))?;

        let visible_bytes = self.visible_bytes();
        let start_index = visible_bytes.start;
        let end_index = visible_bytes.end;
        let visible_bytes_cow = self.data.slice_to_cow(start_index..end_index);

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

    fn map_selections(&mut self, mut f: impl FnMut(SelRegion) -> Vec<SelRegion>) -> HashSet<u16> {
        let mut invalidated_ranges = Vec::new();
        self.selection.map_selections(|region| {
            invalidated_ranges.push(region.min()..=region.max());
            let new = f(region);
            for new_reg in new.iter() {
                invalidated_ranges.push(new_reg.min()..=new_reg.max());
            }
            new
        });
        let mut invalidated_rows = HashSet::new();
        for offset in invalidated_ranges.into_iter().flatten() {
            if let Some(invalidated_row) = self.offset_to_row(offset) {
                invalidated_rows.insert(invalidated_row);
            }
        }
        invalidated_rows
    }

    fn scroll_down(&mut self, stdout: &mut impl Write, line_count: usize) -> Result<()> {
        self.start_offset += 0x10 * line_count;

        if line_count > (self.size.1 - 1) as usize {
            self.draw(stdout)?;
            Ok(())
        } else {
            queue!(stdout, terminal::ScrollUp(line_count as u16))?;
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
            queue!(stdout, terminal::ScrollDown(line_count as u16))?;
            let invalidated_rows = (0..line_count as u16).collect();
            self.draw_rows(stdout, &invalidated_rows) // -1 is statusline
        }
    }

    fn maybe_update_offset(&mut self, stdout: &mut impl Write) -> Result<()> {
        if self.data.is_empty() {
            self.start_offset = 0;
            return Ok(());
        }

        let main_cursor_offset = self.selection.main_cursor_offset();
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

    pub fn apply_delta(&mut self, stdout: &mut impl Write, delta: &RopeDelta) -> Result<()> {
        self.selection.apply_delta(&delta);
        self.data = self.data.apply_delta(&delta);
        self.maybe_update_offset(stdout)?;
        self.draw(stdout)?;
        Ok(())
    }

    pub fn apply_delta_no_cursor_update(
        &mut self,
        stdout: &mut impl Write,
        delta: &RopeDelta,
    ) -> Result<()> {
        self.data = self.data.apply_delta(&delta);
        self.maybe_update_offset(stdout)?;
        self.draw(stdout)?;
        Ok(())
    }

    fn handle_insert_event_default(&mut self, stdout: &mut impl Write, evt: Event) -> Result<()> {
        let (hex, before) = if let State::Insert { hex, before } = self.state {
            (hex, before)
        } else {
            unreachable!()
        };
        match evt {
            Event::Key(KeyEvent {
                code: KeyCode::Char('o'),
                modifiers,
            }) if modifiers.contains(KeyModifiers::CONTROL) => {
                self.state = State::Insert { hex: !hex, before }
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char('n'),
                modifiers,
            }) if modifiers.contains(KeyModifiers::CONTROL) => {
                let inserted_bytes = vec![0];
                let delta = insert(&self.data, &self.selection, inserted_bytes, before);
                if before {
                    self.apply_delta(stdout, &delta)?;
                } else {
                    self.selection.apply_delta_offset_carets(&delta, 1, 0);
                    self.apply_delta_no_cursor_update(stdout, &delta)?;
                }
            }
            Event::Key(KeyEvent {
                code: KeyCode::Backspace,
                ..
            }) => {
                let delta = if before {
                    backspace(&self.data, &self.selection)
                } else {
                    delete_cursor(&self.data, &self.selection)
                };
                self.apply_delta(stdout, &delta)?;
            }
            Event::Key(KeyEvent {
                code: KeyCode::Delete,
                ..
            }) => {
                let delta = delete_cursor(&self.data, &self.selection);
                self.apply_delta(stdout, &delta)?;
            }
            Event::Key(KeyEvent {
                code: KeyCode::Esc, ..
            }) => {
                self.state = State::Normal;
            }
            evt => self.handle_event_default(stdout, evt)?,
        };
        Ok(())
    }

    pub fn run_event_loop(mut self, stdout: &mut impl Write) -> Result<()> {
        execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;

        self.last_draw_time = self.draw(stdout)?;
        terminal::enable_raw_mode()?;
        stdout.flush()?;

        loop {
            match self.state {
                State::Quitting => break,
                State::Normal => {
                    let evt = event::read()?;
                    let start = time::Instant::now();
                    match evt {
                        Event::Key(event) if event.code == KeyCode::Esc => {
                            self.state = State::Quitting;
                        }
                        Event::Key(KeyEvent { code, modifiers })
                            if key_direction(code).is_some() =>
                        {
                            let max_bytes = self.data.len();
                            let bytes_per_line = self.bytes_per_line;
                            let is_extend = modifiers == KeyModifiers::SHIFT;
                            let direction = key_direction(code).unwrap();
                            let invalidated_rows = if is_extend {
                                self.map_selections(|region| {
                                    vec![region.simple_extend(direction, bytes_per_line, max_bytes)]
                                })
                            } else {
                                self.map_selections(|region| {
                                    vec![region.simple_move(direction, bytes_per_line, max_bytes)]
                                })
                            };

                            self.draw_rows(stdout, &invalidated_rows)?;
                            self.maybe_update_offset(stdout)?;
                        }
                        Event::Key(KeyEvent {
                            code: KeyCode::Char('s'),
                            modifiers,
                        }) if modifiers == KeyModifiers::ALT => {
                            self.state = State::Split;
                        }
                        Event::Key(KeyEvent {
                            code: KeyCode::Char(ch),
                            ..
                        }) if ch == 'g' || ch == 'G' => {
                            self.state = State::JumpTo { extend: ch == 'G' };
                        }
                        Event::Key(KeyEvent {
                            code: KeyCode::Char(';'),
                            modifiers,
                        }) if modifiers == KeyModifiers::ALT => {
                            let invalidated_rows =
                                self.map_selections(|region| vec![region.swap_caret()]);
                            self.draw_rows(stdout, &invalidated_rows)?;
                            self.maybe_update_offset(stdout)?;
                        }
                        Event::Key(KeyEvent {
                            code: KeyCode::Char(';'),
                            ..
                        }) => {
                            let invalidated_rows =
                                self.map_selections(|region| vec![region.collapse()]);
                            self.draw_rows(stdout, &invalidated_rows)?;
                        }
                        Event::Key(KeyEvent {
                            code: KeyCode::Char('d'),
                            ..
                        }) => {
                            if !self.data.is_empty() {
                                let delta = deletion(&self.data, &self.selection);
                                self.apply_delta(stdout, &delta)?;
                            }
                        }
                        Event::Key(KeyEvent {
                            code: KeyCode::Char(ch),
                            ..
                        }) if ch == 'i' || ch == 'I' || ch == 'a' || ch == 'A' => {
                            let before = ch.to_ascii_lowercase() == 'i';
                            let invalidated_rows = if before {
                                self.map_selections(|region| vec![region.to_backward()])
                            } else {
                                self.map_selections(|region| vec![region.to_forward()])
                            };
                            self.draw_rows(stdout, &invalidated_rows)?;
                            self.state = State::Insert {
                                before,
                                hex: ch.is_ascii_lowercase(),
                            };
                        }
                        evt => self.handle_event_default(stdout, evt)?,
                    };
                    self.last_draw_time = start.elapsed();
                }
                State::Split => match event::read()? {
                    Event::Key(KeyEvent { code, .. }) if split_width(code).is_some() => {
                        let width = split_width(code).unwrap();
                        let invalidated_rows = self.map_selections(|region| {
                            (region.min()..=region.max())
                                .step_by(width)
                                .map(|pos| {
                                    SelRegion::new(pos, cmp::min(region.max(), pos + width - 1))
                                        .with_direction(region.backward())
                                })
                                .collect()
                        });

                        self.draw_rows(stdout, &invalidated_rows)?;
                        self.state = State::Normal;
                    }
                    Event::Key(_) => self.state = State::Normal,
                    evt => self.handle_event_default(stdout, evt)?,
                },
                State::JumpTo { extend } => match event::read()? {
                    Event::Key(KeyEvent { code, .. }) if key_direction(code).is_some() => {
                        let direction = key_direction(code).unwrap();
                        let max_bytes = self.data.len();
                        let bytes_per_line = self.bytes_per_line;
                        let invalidated_rows = if extend {
                            self.map_selections(|region| {
                                vec![region.extend_to_boundary(
                                    direction,
                                    bytes_per_line,
                                    max_bytes,
                                )]
                            })
                        } else {
                            self.map_selections(|region| {
                                vec![region.jump_to_boundary(direction, bytes_per_line, max_bytes)]
                            })
                        };

                        self.draw_rows(stdout, &invalidated_rows)?;
                        self.maybe_update_offset(stdout)?;
                        self.state = State::Normal;
                    }
                    Event::Key(_) => self.state = State::Normal,
                    evt => self.handle_event_default(stdout, evt)?,
                },
                State::Insert { hex, before } => match event::read()? {
                    Event::Key(KeyEvent {
                        code: KeyCode::Char(ch),
                        modifiers,
                    }) if !hex && modifiers.is_empty() => {
                        let mut inserted_bytes = vec![0u8; ch.len_utf8()];
                        ch.encode_utf8(&mut inserted_bytes);
                        let insertion_len = inserted_bytes.len() as isize;

                        // At this point `before` doesn't really matter;
                        // the cursors will have been moved in normal mode to their
                        // correct places.
                        let delta = insert(&self.data, &self.selection, inserted_bytes, before);
                        if before {
                            self.apply_delta(stdout, &delta)?;
                        } else {
                            self.selection
                                .apply_delta_offset_carets(&delta, insertion_len, 0);
                            self.apply_delta_no_cursor_update(stdout, &delta)?;
                        }
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Char(ch),
                        modifiers,
                    }) if ch.is_ascii_hexdigit() && modifiers.is_empty() => {
                        let inserted = ch.to_digit(16).unwrap() << 4;
                        let mut inserted_bytes = vec![inserted as u8];
                        let insertion_delta =
                            insert(&self.data, &self.selection, inserted_bytes.clone(), before);
                        self.use_half_cursor = true;
                        self.selection.apply_delta_offset_carets(
                            &insertion_delta,
                            if before { -1 } else { 1 },
                            if before { 0 } else { 0 },
                        );
                        self.apply_delta_no_cursor_update(stdout, &insertion_delta)?;
                        stdout.flush()?;

                        self.use_half_cursor = false; // After next update, no more half cursor
                        let next_key_event = loop {
                            match event::read()? {
                                k @ Event::Key(_) => break k,
                                evt => self.handle_insert_event_default(stdout, evt)?,
                            }
                        };

                        if let Event::Key(KeyEvent {
                            code: KeyCode::Char(second_ch),
                            modifiers,
                        }) = next_key_event
                        {
                            if !second_ch.is_ascii_hexdigit() || !modifiers.is_empty() {
                                if before {
                                    // The partial insertion will have extended our selection in the direction
                                    // of the cursor. Fix this up before doing anything.
                                    let bytes_per_line = self.bytes_per_line;
                                    let max_bytes = self.data.len();

                                    let invalidated_rows = self.map_selections(|region| {
                                        vec![region.simple_extend(
                                            Direction::Right,
                                            bytes_per_line,
                                            max_bytes,
                                        )]
                                    });
                                    self.draw_rows(stdout, &invalidated_rows)?;
                                }

                                self.handle_insert_event_default(stdout, next_key_event)?;
                            } else {
                                inserted_bytes[0] |= second_ch.to_digit(16).unwrap() as u8;
                                let delta = change(&self.data, &self.selection, inserted_bytes);

                                self.selection.apply_delta_offset_carets(
                                    &delta,
                                    if before { 0 } else { -1 },
                                    if before { 0 } else { 0 },
                                );
                                self.apply_delta_no_cursor_update(stdout, &delta)?;
                            }
                        } else {
                            if before {
                                // The partial insertion will have extended our selection in the direction
                                // of the cursor. Fix this up before doing anything.
                                let bytes_per_line = self.bytes_per_line;
                                let max_bytes = self.data.len();

                                let invalidated_rows = self.map_selections(|region| {
                                    vec![region.simple_extend(
                                        Direction::Right,
                                        bytes_per_line,
                                        max_bytes,
                                    )]
                                });
                                self.draw_rows(stdout, &invalidated_rows)?;
                            }

                            self.handle_insert_event_default(stdout, next_key_event)?;
                        }
                    }
                    evt => self.handle_insert_event_default(stdout, evt)?,
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
