use serde::Deserialize;
use serde_json as json;

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use alacritty_terminal::ansi;
use alacritty_terminal::config::Config;
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Grid};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::term::{SizeInfo, Term};

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
    selective_erasure
    colored_reset
    delete_lines
    delete_chars_reset
    alt_reset
    deccolm_reset
    decaln_reset
    insert_blank_reset
    erase_chars_reset
    scroll_up_reset
    clear_underline
    region_scroll_down
    wrapline_alt_toggle
    saved_cursor
    saved_cursor_alt
    sgr
    underline
}

fn read_u8<P>(path: P) -> Vec<u8>
where
    P: AsRef<Path>,
{
    let mut res = Vec::new();
    File::open(path.as_ref()).unwrap().read_to_end(&mut res).unwrap();

    res
}

#[derive(Deserialize, Default)]
struct RefConfig {
    history_size: u32,
}

#[derive(Copy, Clone)]
struct Mock;

impl EventListener for Mock {
    fn send_event(&self, _event: Event) {}
}

fn ref_test(dir: &Path) {
    let recording = read_u8(dir.join("alacritty.recording"));
    let serialized_size = fs::read_to_string(dir.join("size.json")).unwrap();
    let serialized_grid = fs::read_to_string(dir.join("grid.json")).unwrap();
    let serialized_cfg = fs::read_to_string(dir.join("config.json")).unwrap();

    let size: SizeInfo = json::from_str(&serialized_size).unwrap();
    let grid: Grid<Cell> = json::from_str(&serialized_grid).unwrap();
    let ref_config: RefConfig = json::from_str(&serialized_cfg).unwrap();

    let mut config = Config::default();
    config.scrolling.set_history(ref_config.history_size);

    let mut terminal = Term::new(&config, size, Mock);
    let mut parser = ansi::Processor::new();

    for byte in recording {
        parser.advance(&mut terminal, byte);
    }

    // Truncate invisible lines from the grid.
    let mut term_grid = terminal.grid().clone();
    term_grid.initialize_all();
    term_grid.truncate();

    if grid != term_grid {
        for i in 0..grid.total_lines() {
            for j in 0..grid.columns() {
                let cell = &term_grid[Line(i as i32)][Column(j)];
                let original_cell = &grid[Line(i as i32)][Column(j)];
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
