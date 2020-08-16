use std::io::{stdout, BufWriter};
use teehee::hex_view::HexView;

const STDOUT_BUF: usize = 8192;

fn main() {
    let stdout = stdout();
    let mut stdout = BufWriter::with_capacity(STDOUT_BUF, stdout.lock());
    let filename = std::env::args().nth(1);
    let contents = filename
        .map(|filename| std::fs::read(&filename).expect("Couldn't read file"))
        .unwrap_or(vec![]);
    let view = HexView::from_data(contents);

    view.run_event_loop(&mut stdout).unwrap();
}
