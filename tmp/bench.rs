#[macro_use]
extern crate criterion;
extern crate alacritty;

use std::io;

use criterion::Criterion;

use alacritty::ansi::{self, ClearMode, Handler};
use alacritty::term::{SizeInfo, Term};

fn setup_term() -> Term {
    // Create buffer with `num_lines` * "y\n"
    let num_lines = 10_000;
    let mut buf = String::with_capacity(num_lines * 2);
    for _ in 0..num_lines {
        buf.push_str("y\n");
    }
    let size = SizeInfo {
        width: 100.,
        height: 100.,
        cell_width: 10.,
        cell_height: 20.,
        padding_x: 0.,
        padding_y: 0.,
    };
    let mut terminal = Term::new(&Default::default(), size);
    let mut parser = ansi::Processor::new();

    for byte in buf.bytes() {
        parser.advance(&mut terminal, byte, &mut io::sink());
    }

    terminal
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("clear_screen", |b| {
        b.iter_with_setup(setup_term, |mut term| term.clear_screen(ClearMode::All))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
