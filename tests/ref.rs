extern crate alacritty;
extern crate serde_json;

/// ref tests
mod reference {
    use std::fs::File;
    use std::io::{self, Read};
    use std::path::Path;

    use serde_json as json;

    use alacritty::Grid;
    use alacritty::Term;
    use alacritty::term::Cell;
    use alacritty::term::SizeInfo;
    use alacritty::ansi;

    /// The /dev/null of io::Write
    struct Void;

    impl io::Write for Void {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    macro_rules! ref_file {
        ($ref_name:ident, $file:expr) => {
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/ref/", stringify!($ref_name), "/", $file
            )
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

    macro_rules! ref_test {
        ($name:ident) => {
            #[test]
            fn $name() {
                let recording = read_u8(ref_file!($name, "alacritty.recording"));
                let serialized_size = read_string(ref_file!($name, "size.json"));
                let serialized_grid = read_string(ref_file!($name, "grid.json"));

                let size: SizeInfo = json::from_str(&serialized_size).unwrap();
                let grid: Grid<Cell> = json::from_str(&serialized_grid).unwrap();

                let mut terminal = Term::new(size);
                let mut parser = ansi::Processor::new();

                for byte in recording {
                    parser.advance(&mut terminal, byte, &mut Void);
                }

                assert_eq!(grid, *terminal.grid());
            }
        };

        ($( $name:ident ),*) => {
            $(
                ref_test! { $name }
            )*
        };
    }

    // Ref tests!
    ref_test! {
        ll,
        vim_simple_edit,
        tmux_htop,
        tmux_git_log,
        vim_large_window_scroll,
        indexed_256_colors,
        fish_cc
    }
}
