use crossterm::style::Attributes;
use crossterm::{
    queue,
    style::{self, Color},
    ErrorKind,
};
use std::fmt;
use std::fmt::Display;
use std::io::Write;

mod byte_properties;
pub mod view;

const COLOR_NULL: Color = Color::AnsiValue(150);
const COLOR_ASCII_PRINTABLE: Color = Color::Cyan;
const COLOR_ASCII_WHITESPACE: Color = Color::Green;
const COLOR_ASCII_OTHER: Color = Color::Rgb {
    r: 232,
    g: 52,
    b: 210,
};
const COLOR_NONASCII: Color = Color::Yellow;

#[derive(Debug, Clone, Copy)]
pub enum Priority {
    Basic,
    #[allow(dead_code)]
    Mark,
    Selection,
    Cursor,
}

#[derive(Debug, Clone)]
pub struct PrioritizedStyle {
    style: style::ContentStyle,
    #[allow(dead_code)]
    priority: Priority,
}

#[derive(Debug, Clone)]
pub struct StylingCommand {
    start: Option<PrioritizedStyle>,
    mid: Option<PrioritizedStyle>,
    end: Option<PrioritizedStyle>,
}

impl Default for StylingCommand {
    fn default() -> Self {
        Self {
            start: Some(PrioritizedStyle {
                style: style::ContentStyle {
                    foreground_color: Some(Color::White),
                    background_color: Some(Color::Reset),
                    attributes: Attributes::default(),
                },
                priority: Priority::Basic,
            }),
            mid: None,
            end: Some(PrioritizedStyle {
                style: style::ContentStyle {
                    foreground_color: Some(Color::White),
                    background_color: Some(Color::Reset),
                    attributes: Attributes::default(),
                },
                priority: Priority::Basic,
            }),
        }
    }
}

impl StylingCommand {
    pub fn start_style(&self) -> Option<&style::ContentStyle> {
        self.start.as_ref().map(|x| &x.style)
    }

    pub fn mid_style(&self) -> Option<&style::ContentStyle> {
        self.mid.as_ref().map(|x| &x.style)
    }

    pub fn end_style(&self) -> Option<&style::ContentStyle> {
        self.end.as_ref().map(|x| &x.style)
    }

    #[must_use]
    pub fn with_start_style(self, style: PrioritizedStyle) -> Self {
        Self {
            start: Some(style),
            ..self
        }
    }

    #[must_use]
    pub fn with_mid_to_end(self) -> Self {
        let StylingCommand { start, mid, .. } = self;
        Self {
            start,
            mid: None,
            end: mid,
        }
    }

    #[must_use]
    pub fn take_end_only(self) -> Self {
        let StylingCommand { end, .. } = self;
        Self {
            start: None,
            mid: None,
            end,
        }
    }

    #[must_use]
    fn with_mid_style(self, style: PrioritizedStyle) -> Self {
        Self {
            mid: Some(style),
            ..self
        }
    }

    #[must_use]
    fn with_end_style(self, style: PrioritizedStyle) -> Self {
        Self {
            end: Some(style),
            ..self
        }
    }
}

fn queue_style(stdout: &mut impl Write, style: &style::ContentStyle) -> Result<(), ErrorKind> {
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

fn colorize_byte(byte: u8, style_cmd: &StylingCommand) -> StylingCommand {
    let default_content_style = style::ContentStyle {
        foreground_color: None,
        background_color: None,
        attributes: Default::default(),
    };

    let start_style = *style_cmd.start_style().unwrap_or(&default_content_style);

    style_cmd.clone().with_start_style(PrioritizedStyle {
        style: style::ContentStyle {
            foreground_color: Some(get_byte_color(byte)),
            background_color: start_style.background_color,
            attributes: start_style.attributes,
        },
        priority: Priority::Basic,
    })
}

pub fn make_padding(len: usize) -> &'static str {
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

pub struct OutputColorizer;

impl OutputColorizer {
    pub fn new() -> Self {
        Self {}
    }

    pub fn draw_hex_byte(
        &self,
        stdout: &mut impl Write,
        byte: u8,
        style: &StylingCommand,
    ) -> Result<(), ErrorKind> {
        let style_cmd = colorize_byte(byte, style);

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

        queue!(stdout, style::Print(" ".to_string()))
    }

    pub fn draw_ascii_byte(
        &self,
        stdout: &mut impl Write,
        byte: u8,
        style: &StylingCommand,
    ) -> Result<(), ErrorKind> {
        let style_cmd = colorize_byte(byte, style);

        if let Some(start_cmd) = style_cmd.start_style() {
            queue_style(stdout, start_cmd)?;
        }

        queue!(stdout, style::Print(format!("{}", ByteAsciiRepr(byte))))?;

        if let Some(end_cmd) = style_cmd.end_style() {
            queue_style(stdout, end_cmd)?;
        }

        Ok(())
    }

    pub fn draw<T: Display>(
        &self,
        stdout: &mut impl Write,
        c: T,
        style: &StylingCommand,
    ) -> Result<(), ErrorKind> {
        if let Some(start_cmd) = style.start_style() {
            queue_style(stdout, start_cmd)?;
        }

        queue!(stdout, style::Print(c))?;

        if let Some(end_cmd) = style.end_style() {
            queue_style(stdout, end_cmd)?;
        }

        Ok(())
    }
}

impl Default for OutputColorizer {
    fn default() -> Self {
        OutputColorizer::new()
    }
}
