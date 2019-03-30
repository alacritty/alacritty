// Copyright 2016 Joe Wilm, The Alacritty Project Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
#[cfg(windows)]
use embed_resource;
#[cfg(windows)]
use reqwest;
#[cfg(windows)]
use tempfile;
#[cfg(windows)]
use zip;

use gl_generator::{Api, Fallbacks, GlobalGenerator, Profile, Registry};

use std::env;
use std::fs::File;
use std::path::Path;

#[cfg(windows)]
use std::fs::OpenOptions;
#[cfg(windows)]
use std::io;

#[cfg(windows)]
const WINPTY_PACKAGE_URL: &str =
    "https://github.com/rprichard/winpty/releases/download/0.4.3/winpty-0.4.3-msvc2015.zip";

fn main() {
    let dest = env::var("OUT_DIR").unwrap();
    let mut file = File::create(&Path::new(&dest).join("gl_bindings.rs")).unwrap();

    Registry::new(Api::Gl, (4, 5), Profile::Core, Fallbacks::All, ["GL_ARB_blend_func_extended"])
        .write_bindings(GlobalGenerator, &mut file)
        .unwrap();

    #[cfg(windows)]
    {
        embed_resource::compile("assets/windows/windows.rc");

        // Path is relative to target/{profile}/build/alacritty-HASH/out
        let file = Path::new(&env::var("OUT_DIR").unwrap()).join("../../../winpty-agent.exe");
        if !file.exists() {
            aquire_winpty_agent(&file);
        }
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
