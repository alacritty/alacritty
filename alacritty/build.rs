use std::env;
use std::fs::File;
use std::path::Path;
use std::process::Command;

use gl_generator::{Api, Fallbacks, GlobalGenerator, Profile, Registry};

fn main() {
    let mut version = String::from(env!("CARGO_PKG_VERSION"));
    if let Some(commit_hash) = commit_hash() {
        version = format!("{version} ({commit_hash})");
    }
    println!("cargo:rustc-env=VERSION={version}");

    let dest = env::var("OUT_DIR").expect("OUT_DIR environment variable should be set");
    let mut file = File::create(Path::new(&dest).join("gl_bindings.rs")).expect("Failed to create gl_bindings.rs file");

    Registry::new(Api::Gl, (3, 3), Profile::Core, Fallbacks::All, [
        "GL_ARB_blend_func_extended",
        "GL_KHR_robustness",
        "GL_KHR_debug",
    ])
    .write_bindings(GlobalGenerator, &mut file)
    .expect("Failed to write GL bindings");

    #[cfg(windows)]
    embed_resource::compile("./windows/alacritty.rc", embed_resource::NONE)
        .manifest_required()
        .expect("Failed to embed Windows resources");
}

fn commit_hash() -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|hash| hash.trim().into())
}
