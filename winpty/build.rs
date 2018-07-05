use std::fs::copy;

fn main() {
    // The working directory for `cargo test` is in the deps folder, not the debug/release root
    #[cfg(debug_assertions)]
    copy("../assets/windows/x86_64/winpty-agent.exe", "target/debug/deps/winpty-agent.exe").unwrap();
    #[cfg(not(debug_assertions))]
    copy("../assets/windows/x86_64/winpty-agent.exe", "target/release/deps/winpty-agent.exe").unwrap();

    println!("cargo:rerun-if-changed=../assets/windows/x86_64/winpty-agent.exe")
}