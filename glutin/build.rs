extern crate gl_generator;

use gl_generator::{Registry, Api, Profile, Fallbacks};
use std::env;
use std::fs::File;
use std::path::PathBuf;

fn main() {
    let target = env::var("TARGET").unwrap();
    let dest = PathBuf::from(&env::var("OUT_DIR").unwrap());

    println!("cargo:rerun-if-changed=build.rs");

    if target.contains("windows") {
        let mut file = File::create(&dest.join("wgl_bindings.rs")).unwrap();
        Registry::new(Api::Wgl, (1, 0), Profile::Core, Fallbacks::All, [])
            .write_bindings(gl_generator::StaticGenerator, &mut file)
            .unwrap();

        let mut file = File::create(&dest.join("wgl_extra_bindings.rs")).unwrap();
        Registry::new(Api::Wgl, (1, 0), Profile::Core, Fallbacks::All, [
                          "WGL_ARB_create_context",
                          "WGL_ARB_create_context_profile",
                          "WGL_ARB_create_context_robustness",
                          "WGL_ARB_context_flush_control",
                          "WGL_ARB_extensions_string",
                          "WGL_ARB_framebuffer_sRGB",
                          "WGL_ARB_multisample",
                          "WGL_ARB_pixel_format",
                          "WGL_ARB_pixel_format_float",
                          "WGL_EXT_create_context_es2_profile",
                          "WGL_EXT_extensions_string",
                          "WGL_EXT_framebuffer_sRGB",
                          "WGL_EXT_swap_control",
                      ])
            .write_bindings(gl_generator::StructGenerator, &mut file).unwrap();

        let mut file = File::create(&dest.join("egl_bindings.rs")).unwrap();
        Registry::new(Api::Egl, (1, 5), Profile::Core, Fallbacks::All, [
                          "EGL_KHR_create_context",
                          "EGL_EXT_create_context_robustness",
                          "EGL_KHR_create_context_no_error",
                          "EGL_KHR_platform_x11",
                          "EGL_KHR_platform_android",
                          "EGL_KHR_platform_wayland",
                          "EGL_KHR_platform_gbm",
                          "EGL_EXT_platform_base",
                          "EGL_EXT_platform_x11",
                          "EGL_MESA_platform_gbm",
                          "EGL_EXT_platform_wayland",
                          "EGL_EXT_platform_device",
                      ])
            .write_bindings(gl_generator::StructGenerator, &mut file).unwrap();
    }

    if target.contains("linux") || target.contains("dragonfly") || target.contains("freebsd") || target.contains("openbsd") {
        let mut file = File::create(&dest.join("glx_bindings.rs")).unwrap();
        Registry::new(Api::Glx, (1, 4), Profile::Core, Fallbacks::All, [])
            .write_bindings(gl_generator::StructGenerator, &mut file).unwrap();

        let mut file = File::create(&dest.join("glx_extra_bindings.rs")).unwrap();
        Registry::new(Api::Glx, (1, 4), Profile::Core, Fallbacks::All, [
                          "GLX_ARB_create_context",
                          "GLX_ARB_create_context_profile",
                          "GLX_ARB_create_context_robustness",
                          "GLX_ARB_context_flush_control",
                          "GLX_ARB_fbconfig_float",
                          "GLX_ARB_framebuffer_sRGB",
                          "GLX_EXT_framebuffer_sRGB",
                          "GLX_ARB_multisample",
                          "GLX_EXT_swap_control",
                          "GLX_SGI_swap_control"
                      ])
            .write_bindings(gl_generator::StructGenerator, &mut file).unwrap();

        let mut file = File::create(&dest.join("egl_bindings.rs")).unwrap();
        Registry::new(Api::Egl, (1, 5), Profile::Core, Fallbacks::All, [
                          "EGL_KHR_create_context",
                          "EGL_EXT_create_context_robustness",
                          "EGL_KHR_create_context_no_error",
                          "EGL_KHR_platform_x11",
                          "EGL_KHR_platform_android",
                          "EGL_KHR_platform_wayland",
                          "EGL_KHR_platform_gbm",
                          "EGL_EXT_platform_base",
                          "EGL_EXT_platform_x11",
                          "EGL_MESA_platform_gbm",
                          "EGL_EXT_platform_wayland",
                          "EGL_EXT_platform_device",
                      ])
            .write_bindings(gl_generator::StructGenerator, &mut file).unwrap();
    }

    if target.contains("android") {
        let mut file = File::create(&dest.join("egl_bindings.rs")).unwrap();
        Registry::new(Api::Egl, (1, 5), Profile::Core, Fallbacks::All, [
                          "EGL_KHR_create_context",
                          "EGL_EXT_create_context_robustness",
                          "EGL_KHR_create_context_no_error",
                          "EGL_KHR_platform_x11",
                          "EGL_KHR_platform_android",
                          "EGL_KHR_platform_wayland",
                          "EGL_KHR_platform_gbm",
                          "EGL_EXT_platform_base",
                          "EGL_EXT_platform_x11",
                          "EGL_MESA_platform_gbm",
                          "EGL_EXT_platform_wayland",
                          "EGL_EXT_platform_device",
                      ])
            .write_bindings(gl_generator::StaticStructGenerator, &mut file).unwrap();
    }

    if target.contains("ios") {
        let mut file = File::create(&dest.join("egl_bindings.rs")).unwrap();
        Registry::new(Api::Egl, (1, 5), Profile::Core, Fallbacks::All, [
                          "EGL_KHR_create_context",
                          "EGL_EXT_create_context_robustness",
                          "EGL_KHR_create_context_no_error",
                          "EGL_KHR_platform_x11",
                          "EGL_KHR_platform_android",
                          "EGL_KHR_platform_wayland",
                          "EGL_KHR_platform_gbm",
                          "EGL_EXT_platform_base",
                          "EGL_EXT_platform_x11",
                          "EGL_MESA_platform_gbm",
                          "EGL_EXT_platform_wayland",
                          "EGL_EXT_platform_device",
                      ])
            .write_bindings(gl_generator::StaticStructGenerator, &mut file).unwrap();

        let mut file = File::create(&dest.join("gles2_bindings.rs")).unwrap();
        Registry::new(Api::Gles2, (2, 0), Profile::Core, Fallbacks::None, [])
            .write_bindings(gl_generator::StaticStructGenerator, &mut file).unwrap();
    }

    if target.contains("darwin") {
        let mut file = File::create(&dest.join("gl_bindings.rs")).unwrap();
        Registry::new(Api::Gl, (3, 2), Profile::Core, Fallbacks::All, ["GL_EXT_framebuffer_object"])
            .write_bindings(gl_generator::GlobalGenerator, &mut file).unwrap();
    }

    // TODO: only build the bindings below if we run tests/examples

    let mut file = File::create(&dest.join("test_gl_bindings.rs")).unwrap();
    Registry::new(Api::Gles2, (3, 0), Profile::Core, Fallbacks::All, [])
            .write_bindings(gl_generator::StructGenerator, &mut file).unwrap();
}
