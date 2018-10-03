#![cfg(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd", target_os = "openbsd"))]

pub use api::egl::ffi::EGLContext;
pub use api::glx::ffi::GLXContext;
pub use platform::RawHandle;

pub use winit::os::unix::XNotSupported;
pub use winit::os::unix::XWindowType;
pub use winit::os::unix::EventsLoopExt;
pub use winit::os::unix::MonitorIdExt;
pub use winit::os::unix::WindowBuilderExt;
pub use winit::os::unix::WindowExt;

use Context;
use os::GlContextExt;

impl GlContextExt for Context {
    type Handle = RawHandle;

    #[inline]
    unsafe fn raw_handle(&self) -> Self::Handle {
        self.context.raw_handle()
    }
}
