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
extern crate git2;
extern crate gl_generator;
extern crate toml;

use gl_generator::{Registry, Api, Profile, Fallbacks, GlobalGenerator};
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

/// Retrieves the git-tag or hash describing the exact version.
/// Errors are not reported.
pub fn get_repo_description<P: AsRef<Path>>(root: P) -> Option<String> {
    let repo = match git2::Repository::discover(root) {
        Ok(r) => r,
        Err(_) => { return None }
    };
	let mut desc_opt = git2::DescribeOptions::new();
    desc_opt.describe_tags().show_commit_oid_as_fallback(true);
    let res = repo.describe(&desc_opt).ok().and_then(|desc| desc.format(None).ok());
    res
}

/// Retrieves a list of direct dependencies from Cargo.lock as a Vec<> of
/// (name, version)-tuples
fn get_build_deps() -> Vec<(String, String)> {
    let mut lock_buf = String::new();
    fs::File::open("Cargo.lock").unwrap().read_to_string(&mut lock_buf).unwrap();
    let lock_toml: toml::Value = lock_buf.parse().unwrap();

    let mut build_deps = Vec::new();
    for package in lock_toml.lookup("root.dependencies").unwrap()
                            .as_slice().unwrap() {
        let mut package_desc = package.as_str().unwrap().split(" ");
        build_deps.push((package_desc.next().unwrap().to_owned(),
                         package_desc.next().unwrap().to_owned()));
    }
    build_deps.sort();
    build_deps
}

/// Retrieves a Vec<> of Strings, describing the enabled features
fn get_features() -> Vec<String> {
    let prefix = "CARGO_FEATURE_";
    let mut features = Vec::new();
    for (name, _) in env::vars() {
        if name.starts_with(&prefix) {
            features.push(name[prefix.len()..].to_owned());
        }
    }
    features
}


fn main() {
    let src = env::var("CARGO_MANIFEST_DIR").unwrap();
    let dest = env::var("OUT_DIR").unwrap();

    // Build the gl_bindings if they do not already exist locally
    if fs::copy(Path::new(&src).join("gl_bindings.rs"),
                Path::new(&dest).join("gl_bindings.rs")).is_err() {
        let mut bindings_file = fs::File::create(&Path::new(&src)
                                                .join("gl_bindings.rs"))
                                .unwrap();
        Registry::new(Api::Gl, (4, 5), Profile::Core, Fallbacks::All, [
                "GL_ARB_blend_func_extended"
            ])
            .write_bindings(GlobalGenerator, &mut bindings_file)
            .unwrap();
        fs::copy(Path::new(&src).join("gl_bindings.rs"),
                 Path::new(&dest).join("gl_bindings.rs")).unwrap();
    }

    // Write various build-time information, to be available in
    // alacritty:build_info
    let mut versions_file = fs::File::create(&Path::new(&dest)
                                             .join("build_info.include"))
                            .unwrap();
    macro_rules! write {
        ($xp:expr) => (
            versions_file.write($xp.as_ref()).unwrap();
        )
    }

    let git_version = match get_repo_description(&src) {
        Some(git_version) => format!("Some(\"{}\");\n", &git_version),
        None => "None;\n".to_owned()
    };
    write!(format!("pub const GIT_VERSION: Option<&'static str> = {}",
                   &git_version));

    macro_rules! write_const_env {
        ($name:ident, $env_name:expr) => {
            write!(format!("pub const {}: &'static str = \"{}\";\n",
                           stringify!($name), env::var($env_name).unwrap()));
        }
    }
    write_const_env!(PKG_VERSION, "CARGO_PKG_VERSION");
    write_const_env!(TARGET, "TARGET");
    write_const_env!(PROFILE, "PROFILE");
    //TODO add the rest, just so we have everything in one place

    let build_deps = get_build_deps();
    write!(format!("pub const DEPENDENCIES: [(&'static str, &'static str); {}] = {:?};\n", build_deps.len(), build_deps));

    let features = get_features();
    write!(format!("pub const FEATURES: [&'static str; {}] = {:?};\n",
                   features.len(), features));
}
