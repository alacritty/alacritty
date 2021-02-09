
use std::borrow::Cow;
#[cfg(not(target_os = "macos"))]
use std::env;
use std::ffi::CStr;
use std::fs::File;
use std::io;
use std::mem::MaybeUninit;

use std::process::{Child, Command, Stdio};
use std::ptr;
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

use libc::{self, c_int, pid_t, winsize, TIOCSCTTY};
use log::error;
// use crate::die;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Passwd<'a> {
    pub name: &'a str,
    pub passwd: &'a str,
    pub uid: libc::uid_t,
    pub gid: libc::gid_t,
    pub gecos: &'a str,
    pub dir: &'a str,
    pub shell: &'a str,
}

/// Return a Passwd struct with pointers into the provided buf.
///
/// # Unsafety
///
/// If `buf` is changed while `Passwd` is alive, bad thing will almost certainly happen.
pub fn get_pw_entry(buf: &mut [i8; 1024]) -> Passwd<'_> {
    // Create zeroed passwd struct.
    let mut entry: MaybeUninit<libc::passwd> = MaybeUninit::uninit();

    let mut res: *mut libc::passwd = ptr::null_mut();

    // Try and read the pw file.
    let uid = unsafe { libc::getuid() };
    let status = unsafe {
        libc::getpwuid_r(uid, entry.as_mut_ptr(), buf.as_mut_ptr() as *mut _, buf.len(), &mut res)
    };
    let entry = unsafe { entry.assume_init() };

    if status < 0 {
        // die!("getpwuid_r failed");
    }

    if res.is_null() {
        // die!("pw not found");
    }

    // Sanity check.
    assert_eq!(entry.pw_uid, uid);

    // Build a borrowed Passwd struct.
    Passwd {
        name: unsafe { CStr::from_ptr(entry.pw_name).to_str().unwrap() },
        passwd: unsafe { CStr::from_ptr(entry.pw_passwd).to_str().unwrap() },
        uid: entry.pw_uid,
        gid: entry.pw_gid,
        gecos: unsafe { CStr::from_ptr(entry.pw_gecos).to_str().unwrap() },
        dir: unsafe { CStr::from_ptr(entry.pw_dir).to_str().unwrap() },
        shell: unsafe { CStr::from_ptr(entry.pw_shell).to_str().unwrap() },
    }
}