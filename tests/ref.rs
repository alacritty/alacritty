extern crate alacritty;
extern crate serde_json as json;

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use alacritty::Grid;
use alacritty::Term;
use alacritty::term::Cell;
use alacritty::term::SizeInfo;
use alacritty::ansi;

macro_rules! ref_tests {
    ($($name:ident,)*) => {
        ref_tests!($($name),*);
    };
    ($($name:ident),*) => {
        $(
            #[test]
            fn $name() {
                let test_dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/ref"));
                let test_path = test_dir.join(stringify!($name));
                ref_test(&test_path);
            }
        )*
    }
}

ref_tests! {
    fish_cc,
    indexed_256_colors,
    ll,
    tmux_git_log,
    tmux_htop,
    vim_large_window_scroll,
    vim_simple_edit,
}

fn read_u8<P>(path: P) -> Vec<u8>
    where P: AsRef<Path>
{
    let mut res = Vec::new();
    File::open(path.as_ref()).unwrap()
        .read_to_end(&mut res).unwrap();

    res
}

fn read_string<P>(path: P) -> String
    where P: AsRef<Path>
{
    let mut res = String::new();
    File::open(path.as_ref()).unwrap()
        .read_to_string(&mut res).unwrap();

    res
}

fn ref_test(dir: &Path) {
    let recording = read_u8(dir.join("alacritty.recording"));
    let serialized_size = read_string(dir.join("size.json"));
    let serialized_grid = read_string(dir.join("grid.json"));

    let size: SizeInfo = json::from_str(&serialized_size).unwrap();
    let grid: Grid<Cell> = json::from_str(&serialized_grid).unwrap();

    let mut terminal = Term::new(&Default::default(), size);
    let mut parser = ansi::Processor::new();

    for byte in recording {
        parser.advance(&mut terminal, byte, &mut io::sink());
    }

    assert_eq!(grid, *terminal.grid());
}
