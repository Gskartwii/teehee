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
    static ref DEFAULT_STYLE: StylingCommand =
        StylingCommand::default().with_start_style(PrioritizedStyle {
            style: style::ContentStyle {
                foreground_color: Some(Color::DarkMagenta),
                background_color: Some(Color::Reset),
                attributes: Attributes::default(),
            },
            priority: Priority::Basic,
        });
    static ref DEFAULT_VALUE_STYLE: StylingCommand =
        StylingCommand::default().with_start_style(PrioritizedStyle {
            style: style::ContentStyle {
                foreground_color: Some(Color::AnsiValue(150)),
                background_color: Some(Color::Reset),
                attributes: Attributes::default(),
            },
            priority: Priority::Basic,
        });
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

fn format_char(c: char) -> String {
    if c.is_ascii_graphic() {
        c.to_string()
    } else {
        format!("{:x?}", c)
    }
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

fn bytes_to_4_byte_vec(data: &[u8]) -> Vec<u8> {
    if data.len() >= 4 {
        data[0..4].to_vec()
    } else {
        let mut res = data.to_vec();
        while res.len() < 4 {
            res.insert(0, 0);
        }
        res
    }
}

fn utf16_into_char(data: &[u8]) -> Result<char, char> {
    if data.len() >= 2 {
        if let Ok(s) = String::from_utf16(&[u16::from_be_bytes([data[0], data[1]])]) {
            return Ok(s.chars().next().unwrap());
        }
    }

    if data.len() >= 4 {
        if let Ok(s) = String::from_utf16(&[
            u16::from_be_bytes([data[0], data[1]]),
            u16::from_be_bytes([data[2], data[3]]),
        ]) {
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
        assert!(data.len() <= 4);
        Self { data, line: 0 }
    }

    pub fn are_all_printed(&self) -> bool {
        self.line > (BytePropertiesFormatter::height() - 1)
    }

    pub fn draw_line(
        &mut self,
        stdout: &mut impl Write,
        colorizer: &OutputColorizer,
    ) -> Result<(), ErrorKind> {
        let first_byte = if !self.data.is_empty() {
            self.data[0]
        } else {
            0
        };

        match self.line {
            0 => {
                colorizer.draw(stdout, "hex u8: ", &DEFAULT_STYLE)?;
                colorizer.draw_hex_byte(
                    stdout,
                    first_byte,
                    &colorize_byte(first_byte, &DEFAULT_VALUE_STYLE),
                )?;

                colorizer.draw(stdout, "          hex u32: ", &DEFAULT_STYLE)?;
                for byte in self.data.iter() {
                    colorizer.draw_hex_byte(
                        stdout,
                        *byte,
                        &colorize_byte(*byte, &DEFAULT_VALUE_STYLE),
                    )?;
                }
            }
            1 => {
                colorizer.draw(stdout, "bin u8: ", &DEFAULT_STYLE)?;
                format_binary_byte(stdout, colorizer, first_byte)?;

                colorizer.draw(stdout, "     bin u32: ", &DEFAULT_STYLE)?;
                for byte in self.data.iter() {
                    format_binary_byte(stdout, colorizer, *byte)?;
                    colorizer.draw(stdout, ' ', &DEFAULT_STYLE)?;
                }
            }
            2 => {
                let byte_literal = format!("{}", first_byte);
                let len = byte_literal.len();

                colorizer.draw(stdout, "dec u8: ", &DEFAULT_STYLE)?;
                colorizer.draw(stdout, byte_literal, &DEFAULT_VALUE_STYLE)?;

                colorizer.draw(stdout, make_padding(12 - len), &DEFAULT_STYLE)?;
                colorizer.draw(stdout, " dec u32: ", &DEFAULT_STYLE)?;
                colorizer.draw(
                    stdout,
                    u32::from_be_bytes(bytes_to_4_byte_vec(self.data).try_into().unwrap()),
                    &DEFAULT_VALUE_STYLE,
                )?;
            }
            3 => {
                let byte_literal = format!("{}", first_byte as i8);
                let len = byte_literal.len();

                colorizer.draw(stdout, "dec i8: ", &DEFAULT_STYLE)?;
                colorizer.draw(stdout, byte_literal, &DEFAULT_VALUE_STYLE)?;

                colorizer.draw(stdout, make_padding(12 - len), &DEFAULT_STYLE)?;
                colorizer.draw(stdout, " dec i32: ", &DEFAULT_STYLE)?;
                colorizer.draw(
                    stdout,
                    i32::from_be_bytes(bytes_to_4_byte_vec(self.data).try_into().unwrap()),
                    &DEFAULT_VALUE_STYLE,
                )?;
            }
            4 => {
                colorizer.draw(stdout, " utf-8: ", &DEFAULT_STYLE)?;
                let len = match utf8_into_char(self.data) {
                    Ok(c) => {
                        let c = format_char(c);
                        let len = c.len();
                        colorizer.draw(stdout, c, &DEFAULT_VALUE_STYLE)?;
                        len
                    }
                    Err(c) => {
                        colorizer.draw(stdout, c, &INVALID_DATA_STYLE)?;
                        1
                    }
                };

                colorizer.draw(stdout, make_padding(12 - len), &DEFAULT_STYLE)?;
                colorizer.draw(stdout, "  utf-16: ", &DEFAULT_STYLE)?;
                match utf16_into_char(self.data) {
                    Ok(c) => colorizer.draw(stdout, format_char(c), &DEFAULT_VALUE_STYLE),
                    Err(c) => colorizer.draw(stdout, c, &INVALID_DATA_STYLE),
                }?;
            }
            _ => (),
        }

        self.line += 1;

        Ok(())
    }

    pub fn height() -> usize {
        5
    }
}

#[cfg(test)]
mod tests {
    use crate::hex_view::byte_properties::utf16_into_char;

    #[test]
    fn test_utf16_into_char() {
        let data = &[0xd8, 0x01, 0xdc, 0x37];
        assert_eq!(utf16_into_char(data), Ok('𐐷'));
    }
}
