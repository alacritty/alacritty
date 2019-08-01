#[macro_use]
extern crate serde_derive;
use serde_json as json;

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use alacritty_terminal::ansi;
use alacritty_terminal::clipboard::Clipboard;
use alacritty_terminal::config::Config;
use alacritty_terminal::index::Column;
use alacritty_terminal::message_bar::MessageBuffer;
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::term::SizeInfo;
use alacritty_terminal::Grid;
use alacritty_terminal::Term;

macro_rules! ref_tests {
    ($($name:ident)*) => {
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
    csi_rep
    fish_cc
    indexed_256_colors
    issue_855
    ll
    newline_with_cursor_beyond_scroll_region
    tab_rendering
    tmux_git_log
    tmux_htop
    vim_24bitcolors_bce
    vim_large_window_scroll
    vim_simple_edit
    vttest_cursor_movement_1
    vttest_insert
    vttest_origin_mode_1
    vttest_origin_mode_2
    vttest_scroll
    vttest_tab_clear_set
    zsh_tab_completion
    history
    grid_reset
    row_reset
    zerowidth
}

fn read_u8<P>(path: P) -> Vec<u8>
where
    P: AsRef<Path>,
{
    let mut res = Vec::new();
    File::open(path.as_ref()).unwrap().read_to_end(&mut res).unwrap();

    res
}

fn read_string<P>(path: P) -> Result<String, ::std::io::Error>
where
    P: AsRef<Path>,
{
    let mut res = String::new();
    File::open(path.as_ref()).and_then(|mut f| f.read_to_string(&mut res))?;

    Ok(res)
}

#[derive(Deserialize, Default)]
struct RefConfig {
    history_size: u32,
}

fn ref_test(dir: &Path) {
    let recording = read_u8(dir.join("alacritty.recording"));
    let serialized_size = read_string(dir.join("size.json")).unwrap();
    let serialized_grid = read_string(dir.join("grid.json")).unwrap();
    let serialized_cfg = read_string(dir.join("config.json")).unwrap_or_default();

    let size: SizeInfo = json::from_str(&serialized_size).unwrap();
    let grid: Grid<Cell> = json::from_str(&serialized_grid).unwrap();
    let ref_config: RefConfig = json::from_str(&serialized_cfg).unwrap_or_default();

    let mut config: Config = Default::default();
    config.scrolling.set_history(ref_config.history_size);

    let mut terminal = Term::new(&config, size, MessageBuffer::new(), Clipboard::new_nop());
    let mut parser = ansi::Processor::new();

    for byte in recording {
        parser.advance(&mut terminal, byte, &mut io::sink());
    }

    // Truncate invisible lines from the grid
    let mut term_grid = terminal.grid().clone();
    term_grid.initialize_all(&Cell::default());
    term_grid.truncate();

    if grid != term_grid {
        for i in 0..grid.len() {
            for j in 0..grid.num_cols().0 {
                let cell = term_grid[i][Column(j)];
                let original_cell = grid[i][Column(j)];
                if original_cell != cell {
                    println!(
                        "[{i}][{j}] {original:?} => {now:?}",
                        i = i,
                        j = j,
                        original = original_cell,
                        now = cell,
                    );
                }
            }
        }

        panic!("Ref test failed; grid doesn't match");
    }

    assert_eq!(grid, term_grid);
}
