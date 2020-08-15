#[cfg(not(any(feature = "x11", feature = "wayland", target_os = "macos", windows)))]
compile_error!(r#"at least one of the "x11"/"wayland" features must be enabled"#);

use gl_generator::{Api, Fallbacks, GlobalGenerator, Profile, Registry};

use std::env;
use std::fs::File;
use std::path::Path;

fn main() {
    let hash = rustc_tools_util::get_commit_hash().unwrap_or_default();
    println!("cargo:rustc-env=GIT_HASH={}", hash);

    let dest = env::var("OUT_DIR").unwrap();
    let mut file = File::create(&Path::new(&dest).join("gl_bindings.rs")).unwrap();

    Registry::new(Api::Gl, (4, 5), Profile::Core, Fallbacks::All, ["GL_ARB_blend_func_extended"])
        .write_bindings(GlobalGenerator, &mut file)
        .unwrap();

    #[cfg(windows)]
    embed_resource::compile("../extra/windows/windows.rc");
}
