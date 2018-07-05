use std::fs::copy;
use std::path::Path;

fn main() {
    // The working directory for `cargo test` is in the deps folder, not the debug/release root
    if cfg!(test) && Path::new("target").exists() {
        #[cfg(debug_assertions)]
        copy("../assets/windows/x86_64/winpty-agent.exe", "target/debug/deps/winpty-agent.exe").unwrap();
        #[cfg(not(debug_assertions))]
        copy("../assets/windows/x86_64/winpty-agent.exe", "target/release/deps/winpty-agent.exe").unwrap();
    }
}