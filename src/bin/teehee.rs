use std::io::{stdout, BufWriter};
use teehee::hex_view::HexView;
use teehee::{Buffer, Buffers};

const STDOUT_BUF: usize = 8192;

fn main() {
    let stdout = stdout();
    let mut stdout = BufWriter::with_capacity(STDOUT_BUF, stdout.lock());
    let filename = std::env::args().nth(1);
    let buffers = filename
        .as_ref()
        .map(|filename| {
            Buffers::with_buffer(Buffer::from_data_and_path(
                std::fs::read(&filename).expect("Couldn't read file"),
                Some(filename),
            ))
        })
        .unwrap_or_else(Buffers::new);
    let view = HexView::with_buffers(buffers);

    view.run_event_loop(&mut stdout).unwrap();
}
