#[macro_use]
extern crate bitflags;
extern crate winpty_sys;

use winpty_sys::*;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;
use std::result::Result;
use std::os::raw::c_void;
use std::ptr::{null, null_mut};
use fmt::{Display, Formatter};

pub enum ErrorCodes {
    Success,
    OutOfMemory,
    SpawnCreateProcessFailed,
    LostConnection,
    AgentExeMissing,
    Unspecified,
    AgentDied,
    AgentTimeout,
    AgentCreationFailed,
}
pub enum MouseMode {
    None,
    Auto,
    Force,
}
bitflags!(
    pub struct SpawnFlags: u64 {
        const AUTO_SHUTDOWN = 0x1;
        const EXIT_AFTER_SHUTDOWN = 0x2;
    }
);
bitflags!(
    pub struct ConfigFlags: u64 {
        const CONERR = 0x1;
        const PLAIN_OUTPUT = 0x2;
        const COLOR_ESCAPES = 0x4;
    }
);

pub struct Handle<'a>(&'a mut c_void);

#[derive(Debug)]
pub struct Err<'a>(&'a mut winpty_error_t, u32, String);
trait PrivateError {
    fn new(*mut winpty_error_t) -> Self;
}
impl<'a> PrivateError for Err<'a> {
    // The error code and message are stored locally after fetching from C because otherwise
    // it's impossible to write conforming implementations of Display and Error
    fn new(e: *mut winpty_error_t) -> Self {
        unsafe {
            let raw = winpty_error_msg(e);
            Err {
                0: &mut *e,
                1: winpty_error_code(e),
                2: String::from_utf16_lossy(std::slice::from_raw_parts(raw, wcslen(raw))),
            }
        }
    }
}
impl<'a> Drop for Err<'a> {
    fn drop(&mut self) {
        unsafe {
            winpty_error_free(self.0);
        }
    }
}
impl<'a> Display for Err<'a> {
    fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error> {
        write!(f, "Code: {}, Message: {}", self.1, self.2)
    }
}
impl<'a> Error for Err<'a> {
    fn description(&self) -> &str {
        &self.2
    }
}

pub struct Config<'a>(&'a mut winpty_config_t);
impl<'a, 'b> Config<'a> {
    pub fn new(flags: ConfigFlags) -> Result<Self, Err<'b>> {
        let mut err = null_mut() as *mut winpty_error_t;
        unsafe {
            winpty_config_new(flags.bits(), &mut err);
            Result::Err(Err::new(err))
        }
    }
    pub fn set_initial_size(&mut self, cols: i32, rows: i32) {
        assert!(cols > 0);
        assert!(rows > 0);
        unsafe {
            winpty_config_set_initial_size(self.0, cols, rows);
        }
    }
    pub fn set_mouse_mode(&mut self, mode: MouseMode) {
        let m = match mode {
            MouseMode::None => 0,
            MouseMode::Auto => 1,
            MouseMode::Force => 2,
        };
        unsafe {
            winpty_config_set_mouse_mode(self.0, m);
        }
    }
    // Might be a better way to represent this while still retaining infinite capability?
    // Enum?
    pub fn set_agent_timeout(&mut self, timeout: u32) {
        unsafe {
            winpty_config_set_agent_timeout(self.0, timeout);
        }
    }
}
impl<'a> Drop for Config<'a> {
    fn drop(&mut self) {
        unsafe {
            winpty_config_free(self.0);
        }
    }
}

pub struct Winpty<'a>(&'a mut winpty_t);
impl<'a, 'b> Winpty<'a> {
    pub fn open(cfg: &Config) -> Result<Self, Err<'b>> {
        let mut err = null_mut() as *mut winpty_error_t;
        unsafe {
            winpty_open(cfg.0, &mut err);
            Result::Err(Err::new(err))
        }
    }
    pub fn handle(&mut self) -> Handle {
        unsafe {
            Handle {
                0: &mut *winpty_agent_process(self.0),
            }
        }
    }
    pub fn conin_name(&mut self) -> PathBuf {
        unsafe {
            let raw = winpty_conin_name(self.0);
            PathBuf::from(&String::from_utf16_lossy(
                std::slice::from_raw_parts(raw, wcslen(raw)),
            ))
        }
    }
    pub fn conout_name(&mut self) -> PathBuf {
        unsafe {
            let raw = winpty_conout_name(self.0);
            PathBuf::from(&String::from_utf16_lossy(
                std::slice::from_raw_parts(raw, wcslen(raw)),
            ))
        }
    }
    pub fn conerr_name(&mut self) -> PathBuf {
        unsafe {
            let raw = winpty_conerr_name(self.0);
            PathBuf::from(&String::from_utf16_lossy(
                std::slice::from_raw_parts(raw, wcslen(raw)),
            ))
        }
    }
    pub fn set_size(&mut self, cols: usize, rows: usize) -> Result<(), Err> {
        let mut err = null_mut() as *mut winpty_error_t;
        unsafe {
            winpty_set_size(self.0, cols as i32, rows as i32, &mut err);
            Result::Err(Err::new(err))
        }
    }
    pub fn console_process_list(&mut self) -> Result<Vec<u32>, Err> {
        unimplemented!();
    }
    // Decide whether this should return a new object and if so should it have the pipe methods
    // This method can return two errors, create_process and the normal one, create an enum?
    pub fn spawn(
        &mut self,
        cfg: &SpawnConfig,
        process_handle: Option<Handle>,
        thread_handle: Option<Handle>,
    ) -> Result<(), Err> {
        let mut err = null_mut() as *mut winpty_error_t;
        let mut p_handle = match process_handle {
            None => null_mut(),
            Some(h) => h.0,
        };
        let mut t_handle = match thread_handle {
            None => null_mut(),
            Some(h) => h.0,
        };
        unsafe {
            winpty_spawn(
                self.0,
                cfg.0 as *const winpty_spawn_config_s,
                &mut p_handle,
                &mut t_handle,
                null_mut(),
                &mut err,
            );
            Result::Err(Err::new(err))
        }
    }
}
unsafe impl<'a> Sync for Winpty<'a> {}
unsafe impl<'a> Send for Winpty<'a> {}
impl<'a> Drop for Winpty<'a> {
    fn drop(&mut self) {
        unsafe {
            winpty_free(self.0);
        }
    }
}

pub struct SpawnConfig<'a>(&'a mut winpty_spawn_config_t);
impl<'a, 'b> SpawnConfig<'a> {
    pub fn new(
        spawnflags: SpawnFlags,
        appname: Option<&str>,
        cmdline: Option<&str>,
        cwd: Option<&str>,
        end: Option<&str>,
    ) -> Result<Self, Err<'b>> {
        let mut err = null_mut() as *mut winpty_error_t;
        // Map a rust string to a raw pointer at the first element of a utf16 array
        let f = |s: &str| s.encode_utf16().collect::<Vec<u16>>()[0] as *const u16;
        unsafe {
            winpty_spawn_config_new(
                spawnflags.bits(),
                appname.map_or(null(), &f),
                cmdline.map_or(null(), &f),
                cwd.map_or(null(), &f),
                end.map_or(null(), &f),
                &mut err,
            );
            Result::Err(Err::new(err))
        }
    }
}
impl<'a> Drop for SpawnConfig<'a> {
    fn drop(&mut self) {
        unsafe {
            winpty_spawn_config_free(self.0);
        }
    }
}
