//! Adapted from the uzers crate (MIT licensed) to fetch precisely the needed information and no more.
use std::ffi::{OsStr, OsString, CStr, CString};
use std::mem;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr;
use std::sync::Arc;

use libc::{uid_t, passwd, c_char};

pub fn by_name<S: AsRef<OsStr> + ?Sized>(username: &S)  -> Option<(Arc<OsStr>, u32, u32, PathBuf)> {
    let username = match CString::new(username.as_ref().as_bytes()) {
        Ok(u) => u,
        Err(_) => {
            // The username that was passed in contained a null character,
            // which will match no usernames.
            return None;
        }
    };

    let mut passwd = unsafe { mem::zeroed::<passwd>() };
    let mut buf = vec![0; 2048];
    let mut result = ptr::null_mut::<passwd>();

    loop {
        let r = unsafe {
            libc::getpwnam_r(
                username.as_ptr(),
                &mut passwd,
                buf.as_mut_ptr(),
                buf.len(),
                &mut result,
            )
        };

        if r != libc::ERANGE {
            break;
        }

        let newsize = buf.len().checked_mul(2)?;
        buf.resize(newsize, 0);
    }

    if result.is_null() {
        // There is no such user, or an error has occurred.
        // errno gets set if there’s an error.
        return None;
    }

    if result != &mut passwd {
        // The result of getpwnam_r should be its input passwd.
        return None;
    }

    let user = unsafe {
        let passwd  = result.read();

        let name  : Arc<OsStr> = from_raw_buf(passwd.pw_name);
        let uid   : u32        = passwd.pw_uid;
        let gid   : u32        = passwd.pw_gid;
        let shell : PathBuf    = from_raw_buf::<OsString>(passwd.pw_shell).into();

        (name, uid, gid, shell)
    };

    Some(user)
}

pub fn current_username() -> Option<OsString> {
    let uid  = unsafe { libc::getuid() };
    let name = by_uid(uid)?;

    Some(
        OsString::from(&*name)
    )
}

fn by_uid(uid: uid_t) -> Option<Arc<OsStr>> {
    let mut passwd = unsafe { mem::zeroed::<passwd>() };
    let mut buf = vec![0; 2048];
    let mut result = ptr::null_mut::<passwd>();

    loop {
        let r =
            unsafe { libc::getpwuid_r(uid, &mut passwd, buf.as_mut_ptr(), buf.len(), &mut result) };

        if r != libc::ERANGE {
            break;
        }

        let newsize = buf.len().checked_mul(2)?;
        buf.resize(newsize, 0);
    }

    if result.is_null() {
        // There is no such user, or an error has occurred.
        // errno gets set if there’s an error.
        return None;
    }

    if result != &mut passwd {
        // The result of getpwuid_r should be its input passwd.
        return None;
    }

    let name = unsafe {
        from_raw_buf(
            result.read().pw_name
        )
    };

    Some(name)
}

unsafe fn from_raw_buf<'a, T>(p: *const c_char) -> T
where
    T: From<&'a OsStr>,
{
    let c_str = CStr::from_ptr(p).to_bytes();

    T::from(
        OsStr::from_bytes(c_str)
    )
}
