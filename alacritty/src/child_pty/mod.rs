#[cfg(not(windows))]
mod unix;
#[cfg(not(windows))]
pub use self::unix::*;


#[cfg(windows)]
pub mod windows;
#[cfg(windows)]
pub use self::windows::*;
#[cfg(windows)]
pub const PTY_BUFFER_SIZE: usize = 0x500;