use std::io::stdout;
use teehee::hex_view::HexView;

fn main() {
    let mut stdout = stdout();
    let filename = std::env::args().skip(1).next().expect("Need a filename");
    let contents = std::fs::read(&filename).expect("Couldn't read file");
    let view = HexView::from_data(contents);

    view.run_event_loop(&mut stdout).unwrap();
}
