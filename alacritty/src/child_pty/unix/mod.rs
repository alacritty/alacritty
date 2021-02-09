use std::{
    ffi::OsStr,
    fs::File,
    os::unix::io::{FromRawFd, RawFd},
    os::unix::process::CommandExt,
    process::{Command, Stdio},
};
use std::os::unix::io::AsRawFd;

use alacritty_terminal::term::SizeInfo;

use std::io;

use nix::sys::termios::{self, InputFlags, SetArg};

use libc::{self, winsize};

use nix::pty::openpty;

use nix::{
    unistd::setsid,
};


use die::die;



pub const PTY_BUFFER_SIZE: usize = 0x500;

mod ioctl {
    nix::ioctl_none_bad!(set_controlling, libc::TIOCSCTTY);
    nix::ioctl_write_ptr_bad!(win_resize, libc::TIOCSWINSZ, libc::winsize);
}





/// Types that can produce a `libc::winsize`.
pub trait ToWinsize {
    /// Get a `libc::winsize`.
    fn to_winsize(&self) -> winsize;
}

impl<'a> ToWinsize for &'a SizeInfo {
    fn to_winsize(&self) -> winsize {
        winsize {
            ws_row: self.screen_lines().0 as libc::c_ushort,
            ws_col: self.cols().0 as libc::c_ushort,
            ws_xpixel: self.width() as libc::c_ushort,
            ws_ypixel: self.height() as libc::c_ushort,
        }
    }
}


pub struct Pty {
    pub fd: RawFd,
    /// The File used by this PTY.
    pub file: File,
    pub fin: File,
    pub slave: i32,
    pub master: i32,
}

impl io::Read for Pty {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let nbytes = self.file.read(buf)?;
        Ok(nbytes)
    }
}

impl std::io::Write for Pty {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.get_file().write_all(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::result::Result<(), std::io::Error> 
    {
        self.get_file().flush()?;
        Ok(())
    }
}


impl Clone for Pty {
    fn clone(&self) -> Pty {
        let fin = unsafe { File::from_raw_fd(self.fd) };
        let file = unsafe { File::from_raw_fd(self.fd) };

        Self {
            fd: self.fd.clone(),
            file,
            fin,
            slave: self.slave,
            master: self.master,
        }
    }
}







impl Pty {

    pub fn get_file(&mut self) -> &mut File {
        &mut self.file
    }
    /// Spawn a process in a new pty.
    pub fn new<I, S>(command: &str, args: I, size: SizeInfo) -> Result<Pty, ()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {

        let new_winsize = winsize {
            ws_row: size.screen_lines().0 as u16,
            ws_col: size.cols().0 as u16,
            ws_xpixel: size.width() as libc::c_ushort,
            ws_ypixel: size.height() as libc::c_ushort,
        };

        let pty = openpty(&new_winsize, None).unwrap();

        if let Ok(mut termios) = termios::tcgetattr(pty.master) {
            // Set character encoding to UTF-8.
            termios.input_flags.set(InputFlags::IUTF8, true);
            let _ = termios::tcsetattr(pty.master, SetArg::TCSANOW, &termios);
        }

        let mut buf = [0; 1024];
        let pw = crate::passwd::get_pw_entry(&mut buf);


        let slave = pty.slave.clone();
        let master = pty.master.clone();

        unsafe {
            Command::new(&command)
                .args(args)
                .stdin(Stdio::from_raw_fd(pty.slave))
                .stdout(Stdio::from_raw_fd(pty.slave))
                .stderr(Stdio::from_raw_fd(pty.slave))
                .env("LOGNAME", pw.name)
                .env("USER", pw.name)
                .env("HOME", pw.dir)
                .env("SHELL", crate::tab_manager::DEFAULT_SHELL)
                .pre_exec(move || {

                    let pid = setsid().map_err(|e| format!("Error occured with setsid: {}", e)).unwrap();
                    if pid.as_raw() == -1 {
                        die!("Failed to set session id: {}", io::Error::last_os_error());
                    }

                    ioctl::set_controlling(slave).unwrap();

                    libc::close(slave);
                    libc::close(master);
        
                    libc::signal(libc::SIGCHLD, libc::SIG_DFL);
                    libc::signal(libc::SIGHUP, libc::SIG_DFL);
                    libc::signal(libc::SIGINT, libc::SIG_DFL);
                    libc::signal(libc::SIGQUIT, libc::SIG_DFL);
                    libc::signal(libc::SIGTERM, libc::SIG_DFL);
                    libc::signal(libc::SIGALRM, libc::SIG_DFL);

                    Ok(())
                })
                .spawn()
                .map_err(|_err| ())
                .and_then(|_ch| {

                    // ch.id

                    let child = Pty {
                        fd: pty.master,
                        file: File::from_raw_fd(pty.master),
                        fin: File::from_raw_fd(pty.master),
                        slave,
                        master
                    };

                    child.resize(new_winsize)?;

                    Ok(child)
                })
        }
    }

    pub fn on_resize(&mut self, size: &SizeInfo) {
        let win = size.to_winsize();

        let new_winsize = winsize {
            ws_row: win.ws_row - 1,
            ws_col: win.ws_col,
            ws_xpixel: win.ws_xpixel,
            ws_ypixel: win.ws_ypixel,
        };

        let res = unsafe { libc::ioctl(self.fd.as_raw_fd(), libc::TIOCSWINSZ, &new_winsize as *const _) };

        if res < 0 {
            die!("ioctl TIOCSWINSZ failed: {}", io::Error::last_os_error());
        }
    }

    /// Send a resize to the process running in this PTY.
    pub fn resize(&self, size: winsize) -> Result<(), ()> {

        let new_winsize = winsize {
            ws_row: size.ws_row - 1,
            ws_col: size.ws_col,
            ws_xpixel: size.ws_xpixel,
            ws_ypixel: size.ws_ypixel,
        };

        unsafe { ioctl::win_resize(self.fd, &new_winsize) }
            .map(|_| ())
            .map_err(|_| ())
    }
}