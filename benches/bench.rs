#[macro_use]
extern crate criterion;
extern crate alacritty;
extern crate serde_json as json;

use std::fs::File;
use std::io::{self, Read};
use std::mem;

use criterion::Criterion;

use alacritty::ansi::{self, ClearMode, Handler, Processor};
use alacritty::config::Config;
use alacritty::grid::{Grid, Scroll};
use alacritty::term::{cell::Cell, SizeInfo, Term};

fn create_term(num_lines: usize) -> (Term, Processor, String) {
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
    let terminal = Term::new(&Default::default(), size);

    let parser = ansi::Processor::new();

    (terminal, parser, buf)
}

fn setup_term(num_lines: usize) -> Term {
    let (mut term, mut parser, buf) = create_term(num_lines);
    for byte in buf.bytes() {
        parser.advance(&mut term, byte, &mut io::sink());
    }
    term
}

fn setup_render_iter() -> (Term, Config) {
    fn read_string(path: &str) -> String {
        let mut buf = String::new();
        File::open(path)
            .and_then(|mut f| f.read_to_string(&mut buf))
            .unwrap();
        buf
    }

    // Need some realistic grid state; using one of the ref files.
    let serialized_grid = read_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/ref/vim_large_window_scroll/grid.json"
    ));
    let serialized_size = read_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/ref/vim_large_window_scroll/size.json"
    ));

    let mut grid: Grid<Cell> = json::from_str(&serialized_grid).unwrap();
    let size: SizeInfo = json::from_str(&serialized_size).unwrap();

    let config = Config::default();

    let mut terminal = Term::new(&config, size);
    mem::swap(terminal.grid_mut(), &mut grid);

    (terminal, config)
}

/// Benchmark for the renderable cells iterator
///
/// The renderable cells iterator yields cells that require work to be
/// displayed (that is, not a an empty background cell). This benchmark
/// measures how long it takes to process the whole iterator.
fn render_iter(args: (Term, Config)) -> (Term, Config) {
    let (term, config) = args;
    {
        let iter = term.renderable_cells(&config, false);
        for cell in iter {
            criterion::black_box(cell);
        }
    }
    (term, config)
}

fn clear_screen(mut term: Term) -> Term {
    term.clear_screen(ClearMode::All);
    term
}

fn scroll_display(mut term: Term) -> Term {
    for _ in 0..10_000 {
        term.scroll_display(Scroll::Lines(1));
    }
    for _ in 0..10_000 {
        term.scroll_display(Scroll::Lines(-1));
    }
    term
}

fn advance_parser(args: (Term, Processor, String)) -> (Term, Processor, String) {
    let (mut term, mut parser, buf) = args;

    for byte in buf.bytes() {
        parser.advance(&mut term, byte, &mut io::sink());
    }

    (term, parser, buf)
}

fn reset_cell(args: (Cell, Cell, usize)) -> (Cell, Cell) {
    let (mut old, new, num_iters) = args;
    for _ in 0..num_iters {
        old.reset(&new);
    }
    (old, new)
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("render_iter", |b| {
        b.iter_with_large_setup(setup_render_iter, render_iter)
    });

    c.bench_function("clear_screen", |b| {
        b.iter_with_large_setup(|| setup_term(100_000), clear_screen)
    });

    c.bench_function("scroll_display", |b| {
        b.iter_with_large_setup(|| setup_term(10_000), scroll_display)
    });

    c.bench_function("advance_parser", |b| {
        b.iter_with_large_setup(|| create_term(1_000), advance_parser)
    });

    c.bench_function("reset_cell", |b| {
        b.iter_with_large_setup(|| (Cell::default(), Cell::default(), 1_000_000), reset_cell)
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
