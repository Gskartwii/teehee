use crate::mode::Mode;
use crate::modes;

use std::io::Write;
use crossterm::queue;
use crossterm::style;
use crossterm::Result;

pub trait StatusLinePrompter: Mode {
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
                        queue!(stdout, style::Print(format!("{:02x} ", byte)))?
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
                            style::style(format!("{:02x}", byte))
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
                    queue!(stdout, style::Print(format!("{}", *byte as char)))?
                }
                PatternPiece::Literal(byte) if normalized_cursor != i => queue!(
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
                    queue!(
                        stdout,
                        style::PrintStyledContent(
                            style::style(format!("{}", *byte as char))
                                .with(style::Color::Black)
                                .on(style::Color::White)
                        ),
                    )?
                }
                PatternPiece::Literal(byte) => queue!(
                    stdout,
                    style::PrintStyledContent(
                        style::style(format!("<{:02x}>", byte))
                            .with(style::Color::Black)
                            .on(style::Color::White)
                    ),
                )?,
                PatternPiece::Wildcard if normalized_cursor != i => queue!(
                    stdout,
                    style::PrintStyledContent(style::style("*").with(style::Color::DarkRed))
                )?,
                PatternPiece::Wildcard => queue!(
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
            queue!(
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
        queue!(
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

        queue!(
            stdout,
            style::Print(
                &self.command
                    [start_column..std::cmp::min(self.command.len(), start_column + max_width)]
            )
        )?;

        if self.cursor == self.command.len() {
            queue!(
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
