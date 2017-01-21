extern crate alacritty;
extern crate serde_json as json;
extern crate test;

use std::env;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use test::{TestDescAndFn, TestDesc, TestFn, ShouldPanic, TestName, test_main};

use alacritty::Grid;
use alacritty::Term;
use alacritty::term::Cell;
use alacritty::term::SizeInfo;
use alacritty::ansi;

fn main() {
    let test_dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/ref"));

    let args = env::args().collect::<Vec<_>>();

    let tests = test_dir
        .read_dir()
        .unwrap()
        .map(|e| desc(e.unwrap().path()))
        .collect();

    test_main(&args, tests);
}

fn desc(dir: PathBuf) -> TestDescAndFn {
    TestDescAndFn {
        desc: TestDesc {
            name: TestName::DynTestName(dir.file_name().unwrap().to_string_lossy().into_owned()),
            ignore: false,
            should_panic: ShouldPanic::No,
        },
        testfn: TestFn::dyn_test_fn(move || ref_test(&dir)),
    }
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

    let mut terminal = Term::new(size);
    let mut parser = ansi::Processor::new();

    for byte in recording {
        parser.advance(&mut terminal, byte, &mut io::sink());
    }

    assert_eq!(grid, *terminal.grid());
}
