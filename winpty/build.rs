#[cfg(windows)]
use std::fs::OpenOptions;
#[cfg(windows)]
use std::io;

#[cfg(windows)]
use std::env;
#[cfg(windows)]
use std::fs::{copy, File};
#[cfg(windows)]
use std::path::Path;

#[cfg(windows)]
use embed_resource;
#[cfg(windows)]
use reqwest;
#[cfg(windows)]
use tempfile;
#[cfg(windows)]
use zip;

#[cfg(windows)]
const WINPTY_PACKAGE_URL: &str =
    "https://github.com/rprichard/winpty/releases/download/0.4.3/winpty-0.4.3-msvc2015.zip";

fn main() {
    #[cfg(windows)]
    {
        embed_resource::compile("../extra/windows/windows.rc");

        // Path is relative to target/{profile}/build/alacritty-HASH/out
        let file = Path::new(&env::var("OUT_DIR").unwrap()).join("../../../winpty-agent.exe");
        if !file.exists() {
            aquire_winpty_agent(&file);
        }

        // The working directory for `cargo test` is in the deps folder, not the debug/release root
        copy(&file, file.parent().unwrap().join("deps/winpty-agent.exe")).unwrap();
    }
}

#[cfg(windows)]
fn aquire_winpty_agent(out_path: &Path) {
    let tmp_dir = tempfile::Builder::new().prefix("alacritty_build").tempdir().unwrap();

    let mut response = reqwest::get(WINPTY_PACKAGE_URL).unwrap();
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(tmp_dir.path().join("winpty_package.zip"))
        .unwrap();

    io::copy(&mut response, &mut file).unwrap();

    let mut archive = zip::ZipArchive::new(file).unwrap();

    let target = match env::var("TARGET").unwrap().split("-").next().unwrap() {
        "x86_64" => "x64",
        "i386" => "ia32",
        _ => panic!("architecture has no winpty binary"),
    };

    let mut winpty_agent = archive.by_name(&format!("{}/bin/winpty-agent.exe", target)).unwrap();

    io::copy(&mut winpty_agent, &mut File::create(out_path).unwrap()).unwrap();
}
