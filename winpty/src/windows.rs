use std::fmt::{self, Display, Formatter};
use std::os::windows::io::RawHandle;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::result::Result;

use winpty_sys::*;

use widestring::WideCString;

#[derive(Copy, Clone, Debug)]
pub enum ErrorCode {
    OutOfMemory,
    SpawnCreateProcessFailed,
    LostConnection,
    AgentExeMissing,
    Unspecified,
    AgentDied,
    AgentTimeout,
    AgentCreationFailed,
    UnknownError(u32),
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

#[derive(Debug)]
pub struct Error {
    code: ErrorCode,
    message: String,
}

// Check to see whether winpty gave us an error, and perform the necessary memory freeing
fn check_err(e: *mut winpty_error_t) -> Result<(), Error> {
    unsafe {
        let code = winpty_error_code(e);
        let raw = winpty_error_msg(e);
        let message = String::from_utf16_lossy(std::slice::from_raw_parts(raw, wcslen(raw)));
        winpty_error_free(e);

        let code = match code {
            WINPTY_ERROR_SUCCESS => return Ok(()),
            WINPTY_ERROR_OUT_OF_MEMORY => ErrorCode::OutOfMemory,
            WINPTY_ERROR_SPAWN_CREATE_PROCESS_FAILED => ErrorCode::SpawnCreateProcessFailed,
            WINPTY_ERROR_LOST_CONNECTION => ErrorCode::LostConnection,
            WINPTY_ERROR_AGENT_EXE_MISSING => ErrorCode::AgentExeMissing,
            WINPTY_ERROR_UNSPECIFIED => ErrorCode::Unspecified,
            WINPTY_ERROR_AGENT_DIED => ErrorCode::AgentDied,
            WINPTY_ERROR_AGENT_TIMEOUT => ErrorCode::AgentTimeout,
            WINPTY_ERROR_AGENT_CREATION_FAILED => ErrorCode::AgentCreationFailed,
            code => ErrorCode::UnknownError(code),
        };

        Err(Error { code, message })
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error> {
        write!(f, "Code: {:?}, Message: {}", self.code, self.message)
    }
}

impl std::error::Error for Error {
    fn description(&self) -> &str {
        &self.message
    }
}

#[derive(Debug)]
/// Winpty agent config
pub struct Config(*mut winpty_config_t);

impl Config {
    pub fn new(flags: ConfigFlags) -> Result<Self, Error> {
        let mut err = null_mut() as *mut winpty_error_t;
        let config = unsafe { winpty_config_new(flags.bits(), &mut err) };
        check_err(err)?;
        Ok(Self(config))
    }

    /// Set the initial size of the console window
    pub fn set_initial_size(&mut self, cols: i32, rows: i32) {
        unsafe {
            winpty_config_set_initial_size(self.0, cols, rows);
        }
    }

    /// Set the mouse mode
    pub fn set_mouse_mode(&mut self, mode: &MouseMode) {
        let m = match mode {
            MouseMode::None => WINPTY_MOUSE_MODE_NONE,
            MouseMode::Auto => WINPTY_MOUSE_MODE_AUTO,
            MouseMode::Force => WINPTY_MOUSE_MODE_FORCE,
        };
        unsafe {
            winpty_config_set_mouse_mode(self.0, m as i32);
        }
    }

    /// Amount of time to wait for the agent to startup and to wait for any given
    /// agent RPC request.  Must be greater than 0.  Can be INFINITE.
    // Might be a better way to represent this while still retaining infinite capability?
    // Enum?
    pub fn set_agent_timeout(&mut self, timeout: u32) {
        unsafe {
            winpty_config_set_agent_timeout(self.0, timeout);
        }
    }
}

impl Drop for Config {
    fn drop(&mut self) {
        unsafe {
            winpty_config_free(self.0);
        }
    }
}

#[derive(Debug)]
/// A struct representing the winpty agent process
pub struct Winpty(*mut winpty_t);

pub struct ChildHandles {
    pub process: HANDLE,
    pub thread: HANDLE,
}

impl Winpty {
    /// Starts the agent. This process will connect to the agent
    /// over a control pipe, and the agent will open data pipes
    /// (e.g. CONIN and CONOUT).
    pub fn open(cfg: &Config) -> Result<Self, Error> {
        let mut err = null_mut() as *mut winpty_error_t;
        let winpty = unsafe { winpty_open(cfg.0, &mut err) };
        check_err(err)?;
        Ok(Self(winpty))
    }

    /// Returns the handle to the winpty agent process
    pub fn raw_handle(&mut self) -> RawHandle {
        unsafe { winpty_agent_process(self.0) }
    }

    /// Returns the name of the input pipe.
    /// Pipe is half-duplex.
    pub fn conin_name(&mut self) -> PathBuf {
        unsafe {
            let raw = winpty_conin_name(self.0);
            PathBuf::from(&String::from_utf16_lossy(std::slice::from_raw_parts(raw, wcslen(raw))))
        }
    }

    /// Returns the name of the output pipe.
    /// Pipe is half-duplex.
    pub fn conout_name(&mut self) -> PathBuf {
        unsafe {
            let raw = winpty_conout_name(self.0);
            PathBuf::from(&String::from_utf16_lossy(std::slice::from_raw_parts(raw, wcslen(raw))))
        }
    }

    /// Returns the name of the error pipe.
    /// The name will only be valid if ConfigFlags::CONERR was specified.
    /// Pipe is half-duplex.
    pub fn conerr_name(&mut self) -> PathBuf {
        unsafe {
            let raw = winpty_conerr_name(self.0);
            PathBuf::from(&String::from_utf16_lossy(std::slice::from_raw_parts(raw, wcslen(raw))))
        }
    }

    /// Change the size of the Windows console window.
    ///
    /// cols & rows MUST be greater than 0
    pub fn set_size(&mut self, cols: u16, rows: u16) -> Result<(), Error> {
        assert!(cols > 0 && rows > 0);
        let mut err = null_mut() as *mut winpty_error_t;

        unsafe {
            winpty_set_size(self.0, i32::from(cols), i32::from(rows), &mut err);
        }

        check_err(err)
    }

    /// Get the list of processes running in the winpty agent. Returns <= count processes
    ///
    /// `count` must be greater than 0. Larger values cause a larger allocation.
    // TODO: This should return Vec<Handle> instead of Vec<i32>
    pub fn console_process_list(&mut self, count: usize) -> Result<Vec<i32>, Error> {
        assert!(count > 0);

        let mut err = null_mut() as *mut winpty_error_t;
        let mut process_list = Vec::with_capacity(count);

        unsafe {
            let len = winpty_get_console_process_list(
                self.0,
                process_list.as_mut_ptr(),
                count as i32,
                &mut err,
            ) as usize;
            process_list.set_len(len);
        }

        check_err(err)?;
        Ok(process_list)
    }

    /// Spawns the new process.
    ///
    /// spawn can only be called once per Winpty object.  If it is called
    /// before the output data pipe(s) is/are connected, then collected output is
    /// buffered until the pipes are connected, rather than being discarded.
    /// (https://blogs.msdn.microsoft.com/oldnewthing/20110107-00/?p=11803)
    pub fn spawn(&mut self, cfg: &SpawnConfig) -> Result<ChildHandles, Error> {
        let mut handles =
            ChildHandles { process: std::ptr::null_mut(), thread: std::ptr::null_mut() };

        let mut create_process_error: DWORD = 0;
        let mut err = null_mut() as *mut winpty_error_t;

        unsafe {
            winpty_spawn(
                self.0,
                cfg.0 as *const winpty_spawn_config_s,
                &mut handles.process as *mut _,
                &mut handles.thread as *mut _,
                &mut create_process_error as *mut _,
                &mut err,
            );
        }

        let mut result = check_err(err);
        if let Err(Error { code: ErrorCode::SpawnCreateProcessFailed, message }) = &mut result {
            *message = format!("{} (error code {})", message, create_process_error);
        }
        result.map(|_| handles)
    }
}

// winpty_t is thread-safe
unsafe impl Sync for Winpty {}
unsafe impl Send for Winpty {}

impl Drop for Winpty {
    fn drop(&mut self) {
        unsafe {
            winpty_free(self.0);
        }
    }
}

#[derive(Debug)]
/// Information about a process for winpty to spawn
pub struct SpawnConfig(*mut winpty_spawn_config_t);

impl SpawnConfig {
    /// Creates a new spawnconfig
    pub fn new(
        spawnflags: SpawnFlags,
        appname: Option<&str>,
        cmdline: Option<&str>,
        cwd: Option<&str>,
        end: Option<&str>,
    ) -> Result<Self, Error> {
        let mut err = null_mut() as *mut winpty_error_t;

        let to_wstring = |s| WideCString::from_str(s).unwrap();
        let appname = appname.map(to_wstring);
        let cmdline = cmdline.map(to_wstring);
        let cwd = cwd.map(to_wstring);
        let end = end.map(to_wstring);

        let wstring_ptr = |opt: &Option<WideCString>| opt.as_ref().map_or(null(), |ws| ws.as_ptr());
        let spawn_config = unsafe {
            winpty_spawn_config_new(
                spawnflags.bits(),
                wstring_ptr(&appname),
                wstring_ptr(&cmdline),
                wstring_ptr(&cwd),
                wstring_ptr(&end),
                &mut err,
            )
        };

        check_err(err)?;
        Ok(Self(spawn_config))
    }
}

impl Drop for SpawnConfig {
    fn drop(&mut self) {
        unsafe {
            winpty_spawn_config_free(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use named_pipe::PipeClient;
    use winapi::um::processthreadsapi::OpenProcess;
    use winapi::um::winnt::READ_CONTROL;

    use crate::{Config, ConfigFlags, SpawnConfig, SpawnFlags, Winpty};

    #[test]
    // Test that we can start a process in winpty
    fn spawn_process() {
        let mut winpty =
            Winpty::open(&Config::new(ConfigFlags::empty()).expect("failed to create config"))
                .expect("failed to create winpty instance");

        winpty
            .spawn(
                &SpawnConfig::new(SpawnFlags::empty(), None, Some("cmd"), None, None)
                    .expect("failed to create spawn config"),
            )
            .unwrap();
    }

    #[test]
    // Test that pipes connected before winpty is spawned can be connected to
    fn valid_pipe_connect_before() {
        let mut winpty =
            Winpty::open(&Config::new(ConfigFlags::empty()).expect("failed to create config"))
                .expect("failed to create winpty instance");

        // Check we can connect to both pipes
        PipeClient::connect_ms(winpty.conout_name(), 1000)
            .expect("failed to connect to conout pipe");
        PipeClient::connect_ms(winpty.conin_name(), 1000).expect("failed to connect to conin pipe");

        winpty
            .spawn(
                &SpawnConfig::new(SpawnFlags::empty(), None, Some("cmd"), None, None)
                    .expect("failed to create spawn config"),
            )
            .unwrap();
    }

    #[test]
    // Test that pipes connected after winpty is spawned can be connected to
    fn valid_pipe_connect_after() {
        let mut winpty =
            Winpty::open(&Config::new(ConfigFlags::empty()).expect("failed to create config"))
                .expect("failed to create winpty instance");

        winpty
            .spawn(
                &SpawnConfig::new(SpawnFlags::empty(), None, Some("cmd"), None, None)
                    .expect("failed to create spawn config"),
            )
            .unwrap();

        // Check we can connect to both pipes
        PipeClient::connect_ms(winpty.conout_name(), 1000)
            .expect("failed to connect to conout pipe");
        PipeClient::connect_ms(winpty.conin_name(), 1000).expect("failed to connect to conin pipe");
    }

    #[test]
    fn resize() {
        let mut winpty =
            Winpty::open(&Config::new(ConfigFlags::empty()).expect("failed to create config"))
                .expect("failed to create winpty instance");

        winpty
            .spawn(
                &SpawnConfig::new(SpawnFlags::empty(), None, Some("cmd"), None, None)
                    .expect("failed to create spawn config"),
            )
            .unwrap();

        winpty.set_size(1, 1).unwrap();
    }

    #[test]
    #[ignore]
    // Test that each id returned by cosole_process_list points to an actual process
    fn console_process_list_valid() {
        let mut winpty =
            Winpty::open(&Config::new(ConfigFlags::empty()).expect("failed to create config"))
                .expect("failed to create winpty instance");

        winpty
            .spawn(
                &SpawnConfig::new(SpawnFlags::empty(), None, Some("cmd"), None, None)
                    .expect("failed to create spawn config"),
            )
            .unwrap();

        let processes =
            winpty.console_process_list(1000).expect("failed to get console process list");

        // Check that each id is valid
        processes.iter().for_each(|id| {
            let handle = unsafe {
                OpenProcess(
                    READ_CONTROL, // permissions
                    false as i32, // inheret
                    *id as u32,
                )
            };
            assert!(!handle.is_null());
        });
    }
}
