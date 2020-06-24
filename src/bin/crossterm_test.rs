use std::cmp;
use std::fmt;
use std::io::{stdout, Write};
use crossterm::{
    execute, queue,
    event, cursor, terminal, style, Result,
    event::{KeyCode, Event},
};

const BYTES_PER_LINE: usize = 0x10;
const DATA: &[u8] = &[
	0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
	16, 17, 18, 19, 0xFF, 64, 65, 0x64, 0x65, 0x66,
];

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

struct HexView {
	data: Vec<u8>,
}

impl HexView {
    fn from_data(data: Vec<u8>) -> HexView {
        HexView{data}
    }

    fn hex_digits_in_offset(&self) -> usize {
    	let bits_in_offset = (32 - (self.data.len() as u32).leading_zeros()) as usize;
    	let hex_digits_in_offset = (bits_in_offset + 3) / 4;

    	hex_digits_in_offset
    }

    fn draw<W: Write>(&self, stdout: &mut W) -> Result<()> {
		let hex_digits_in_offset = self.hex_digits_in_offset();
		for i in (0..self.data.len()).step_by(BYTES_PER_LINE) {
    		let end = cmp::min(self.data.len(), i + BYTES_PER_LINE);
    		queue!(
        		stdout,
        		cursor::MoveTo(0, (i/BYTES_PER_LINE) as u16),
        		style::Print(" ".to_string()), // Padding
        		style::Print(format!(
            		"{:>1$x}", i, hex_digits_in_offset,
            	))
        	)?;

        	queue!(
            	stdout,
            	style::Print(" | "),
        	)?;

        	let bytes = &self.data[i .. end];
        	for byte in bytes.iter().copied() {
				queue!(
    				stdout,
    				style::Print(format!(
						"{:02x} ", byte,
    				))
				)?;
        	}

        	queue!(
            	stdout,
            	cursor::MoveTo((hex_digits_in_offset + 3 + 3*BYTES_PER_LINE) as u16, (i/BYTES_PER_LINE) as u16),
            	style::Print("| "),
        	)?;

        	for byte in bytes.iter().copied() {
				queue!(
    				stdout,
    				style::Print(format!(
						"{}", ByteAsciiRepr(byte),
    				))
				)?;
        	}
		}

		stdout.flush()?;
		Ok(())
    }
}

fn main() {
    let mut stdout = stdout();

    execute!(
        stdout,
        terminal::EnterAlternateScreen,
        terminal::Clear(terminal::ClearType::All),
    ).unwrap();
	let view = HexView::from_data(DATA.to_vec());

	view.draw(&mut stdout);

	terminal::enable_raw_mode().unwrap();
	loop {
		match event::read().unwrap() {
			Event::Key(event) if event.code == KeyCode::Char('q') => break,
			_ => {},
		}
	}
	terminal::disable_raw_mode().unwrap();

	execute!(
    	stdout,
    	terminal::LeaveAlternateScreen,
	).unwrap();
}
