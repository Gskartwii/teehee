use crate::hex_view::{OutputColorizer, StylingCommand, colorize_byte, PrioritizedStyle, Priority};
use crossterm::{ErrorKind, style};
use std::io::Write;
use crossterm::style::{Color, Attributes};
use lazy_static::lazy_static;

lazy_static! {
    static ref BIN_ZERO_STYLE: StylingCommand = StylingCommand::default().with_start_style(PrioritizedStyle {
        style: style::ContentStyle {
            foreground_color: Some(Color::Green),
            background_color: Some(Color::Reset),
            attributes: Attributes::default(),
        },
        priority: Priority::Basic,
    });

    static ref BIN_ONE_STYLE: StylingCommand = StylingCommand::default().with_start_style(PrioritizedStyle {
        style: style::ContentStyle {
            foreground_color: Some(Color::Red),
            background_color: Some(Color::Reset),
            attributes: Attributes::default(),
        },
        priority: Priority::Basic,
    });
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
        self.line > 2
    }

    pub fn draw_line(
        &mut self,
        stdout: &mut impl Write,
        colorizer: &OutputColorizer,
    ) -> Result<(), ErrorKind> {
        let default_style = StylingCommand::default();

        match self.line {
            0 => {
                colorizer.draw(stdout, "hex: ", &default_style)?;
                colorizer.draw_hex_byte(stdout, self.data[0], &colorize_byte(self.data[0], &default_style))?;
            }
            1 => {
                colorizer.draw(stdout, "binary: ", &default_style)?;
                for c in format!("{:08b}", self.data[0]).chars() {
                    match c {
                        '0' => colorizer.draw(stdout, '0', &BIN_ZERO_STYLE)?,
                        '1' => colorizer.draw(stdout, '1', &BIN_ONE_STYLE)?,
                        _ => {},
                    }
                }
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
        2
    }
}
