use crate::hex_view::{
    colorize_byte, make_padding, OutputColorizer, PrioritizedStyle, Priority, StylingCommand,
};
use crossterm::style::{Attributes, Color};
use crossterm::{style, ErrorKind};
use lazy_static::lazy_static;
use std::convert::TryInto;
use std::io::Write;

lazy_static! {
    static ref BIN_ZERO_STYLE: StylingCommand =
        StylingCommand::default().with_start_style(PrioritizedStyle {
            style: style::ContentStyle {
                foreground_color: Some(Color::AnsiValue(150)),
                background_color: Some(Color::Reset),
                attributes: Attributes::default(),
            },
            priority: Priority::Basic,
        });
    static ref BIN_ONE_STYLE: StylingCommand =
        StylingCommand::default().with_start_style(PrioritizedStyle {
            style: style::ContentStyle {
                foreground_color: Some(Color::Blue),
                background_color: Some(Color::Reset),
                attributes: Attributes::default(),
            },
            priority: Priority::Basic,
        });
    static ref INVALID_DATA_STYLE: StylingCommand =
        StylingCommand::default().with_start_style(PrioritizedStyle {
            style: style::ContentStyle {
                foreground_color: Some(Color::Red),
                background_color: Some(Color::Reset),
                attributes: Attributes::default(),
            },
            priority: Priority::Basic,
        });
    static ref DEFAULT_STYLE: StylingCommand = StylingCommand::default();
}

fn format_binary_byte(
    stdout: &mut impl Write,
    colorizer: &OutputColorizer,
    byte: u8,
) -> Result<(), ErrorKind> {
    for c in format!("{:08b}", byte).chars() {
        match c {
            '0' => colorizer.draw(stdout, '0', &BIN_ZERO_STYLE)?,
            '1' => colorizer.draw(stdout, '1', &BIN_ONE_STYLE)?,
            _ => {}
        }
    }
    Ok(())
}

fn utf8_into_char(data: &[u8]) -> Result<char, char> {
    let max_char_len = if data.len() < 4 { data.len() } else { 4 };

    for i in 1..=max_char_len {
        if let Ok(s) = String::from_utf8(data[0..i].to_vec()) {
            return Ok(s.chars().next().unwrap());
        }
    }

    Err('�')
}

fn utf16_into_char(data: &[u8]) -> Result<char, char> {
    let max_char_len = if data.len() < 4 { data.len() } else { 4 };

    for i in (2..=max_char_len).step_by(2) {
        if let Ok(s) = String::from_utf16(&(0..i).map(|i| u16::from_be_bytes([data[2*i], data[2*i+1]])).collect::<Vec<_>>()) {
            return Ok(s.chars().next().unwrap());
        }
    }

    Err('�')
}

pub struct BytePropertiesFormatter<'a> {
    data: &'a [u8],
    line: usize,
}

impl<'a> BytePropertiesFormatter<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, line: 0 }
    }

    pub fn are_all_printed(&self) -> bool {
        self.line > (BytePropertiesFormatter::height() + 1) as usize
    }

    pub fn draw_line(
        &mut self,
        stdout: &mut impl Write,
        colorizer: &OutputColorizer,
    ) -> Result<(), ErrorKind> {
        match self.line {
            0 => {
                colorizer.draw(stdout, "hex u8: ", &DEFAULT_STYLE)?;
                colorizer.draw_hex_byte(
                    stdout,
                    self.data[0],
                    &colorize_byte(self.data[0], &DEFAULT_STYLE),
                )?;
                colorizer.draw(stdout, "      hex u32: ", &DEFAULT_STYLE)?;
                for byte in self.data[0..4].iter() {
                    colorizer.draw_hex_byte(
                        stdout,
                        *byte,
                        &colorize_byte(*byte, &DEFAULT_STYLE),
                    )?;
                }
            }
            1 => {
                colorizer.draw(stdout, "bin u8: ", &DEFAULT_STYLE)?;
                format_binary_byte(stdout, colorizer, self.data[0]);
                colorizer.draw(stdout, " bin u32: ", &DEFAULT_STYLE)?;
                for byte in self.data[0..4].iter() {
                    format_binary_byte(stdout, colorizer, *byte);
                    colorizer.draw(stdout, ' ', &DEFAULT_STYLE);
                }
            }
            2 => {
                let byte_literal = format!("{}", self.data[0]);
                let len = byte_literal.len();

                colorizer.draw(stdout, "dec u8: ", &DEFAULT_STYLE)?;
                colorizer.draw(stdout, byte_literal, &DEFAULT_STYLE)?;

                colorizer.draw(stdout, make_padding(8 - len), &DEFAULT_STYLE);
                colorizer.draw(stdout, " dec u32: ", &DEFAULT_STYLE);
                colorizer.draw(
                    stdout,
                    u32::from_be_bytes(self.data[0..4].try_into().unwrap()),
                    &DEFAULT_STYLE,
                )?;
            }
            3 => {
                let byte_literal = format!("{}", self.data[0] as i8);
                let len = byte_literal.len();

                colorizer.draw(stdout, "dec i8: ", &DEFAULT_STYLE)?;
                colorizer.draw(stdout, byte_literal, &DEFAULT_STYLE)?;

                colorizer.draw(stdout, make_padding(8 - len), &DEFAULT_STYLE);
                colorizer.draw(stdout, " dec i32: ", &DEFAULT_STYLE);
                colorizer.draw(
                    stdout,
                    i32::from_be_bytes(self.data[0..4].try_into().unwrap()),
                    &DEFAULT_STYLE,
                )?;
            }
            4 => {
                colorizer.draw(stdout, " utf-8: ", &DEFAULT_STYLE)?;
                match utf8_into_char(self.data) {
                    Ok(c) => colorizer.draw(stdout, c, &DEFAULT_STYLE),
                    Err(c) => colorizer.draw(stdout, c, &INVALID_DATA_STYLE),
                }?;

                colorizer.draw(stdout, "         utf-16: ", &DEFAULT_STYLE)?;
                match utf16_into_char(self.data) {
                    Ok(c) => colorizer.draw(stdout, c, &DEFAULT_STYLE),
                    Err(c) => colorizer.draw(stdout, c, &INVALID_DATA_STYLE),
                }?;
            }
            _ => {
                return Err(ErrorKind::new(
                    std::io::ErrorKind::Other,
                    "All needed lines are printed",
                ))
            }
        }

        self.line += 1;

        Ok(())
    }

    pub fn height() -> u16 {
        5
    }
}
