//! Fail closed when mode bits do not establish the private-state access policy.
#[cfg(any(target_os = "macos", target_os = "linux"))]
use anyhow::Context;
use anyhow::Result;
use std::fs::File;

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use anyhow::ensure;
    use std::{
        ffi::{c_int, c_void},
        os::fd::AsRawFd,
    };
    // Darwin sys/acl.h: opaque handles, ACL_TYPE_EXTENDED=0x100, DENY=2.
    unsafe extern "C" {
        fn acl_get_fd_np(fd: c_int, kind: c_int) -> *mut c_void;
        fn acl_valid(acl: *mut c_void) -> c_int;
        fn acl_get_entry(acl: *mut c_void, index: c_int, entry: *mut *mut c_void) -> c_int;
        fn acl_get_tag_type(entry: *mut c_void, tag: *mut c_int) -> c_int;
        fn acl_free(object: *mut c_void) -> c_int;
    }
    struct Acl(*mut c_void);
    impl Drop for Acl {
        fn drop(&mut self) {
            // SAFETY: sole owner of the non-null allocation returned by acl_get_fd_np.
            // There is no actionable recovery for an allocator failure during Drop.
            let _ = unsafe { acl_free(self.0) };
        }
    }
    pub(super) fn check(file: &File) -> Result<()> {
        // SAFETY: live descriptor and the documented Darwin extended-ACL selector.
        let raw = unsafe { acl_get_fd_np(file.as_raw_fd(), 0x100) };
        if raw.is_null() {
            let error = std::io::Error::last_os_error();
            // On a live Darwin descriptor ENOENT means no extended ACL is present.
            // Do not treat unsupported retrieval or permission errors as absence.
            if error.raw_os_error() == Some(libc::ENOENT) {
                return Ok(());
            }
            return Err(error).context("inspect private-state ACL");
        }
        let acl = Acl(raw);
        // SAFETY: the owned ACL handle is live for the whole inspection.
        ensure!(
            unsafe { acl_valid(acl.0) } == 0,
            "Invalid private-state ACL"
        );
        for index in 0..=128 {
            let mut entry = std::ptr::null_mut();
            // SAFETY: valid ACL, bounded nonnegative index and writable entry output.
            let status = unsafe { acl_get_entry(acl.0, index, &mut entry) };
            if status == -1 {
                // Darwin returns -1/EINVAL at end (unlike Linux's POSIX API).
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() == Some(libc::EINVAL) {
                    return Ok(());
                }
                return Err(error).context("read private-state ACL entry");
            }
            ensure!(
                status == 0 && index < 128 && !entry.is_null(),
                "Unsupported private-state ACL"
            );
            let mut tag = 0;
            // SAFETY: entry belongs to the live ACL and tag is a writable C enum.
            ensure!(
                unsafe { acl_get_tag_type(entry, &mut tag) } == 0 && tag == 2,
                "Private state or its ancestors contain an allow ACL; owner action required"
            );
        }
        anyhow::bail!("Private-state ACL entry limit")
    }
}

#[cfg(target_os = "macos")]
pub(super) fn check(file: &File) -> Result<()> {
    macos::check(file)
}

#[cfg(target_os = "linux")]
pub(super) fn check(file: &File) -> Result<()> {
    use std::os::fd::AsRawFd;
    let names = [c"system.posix_acl_access", c"system.posix_acl_default"];
    let count = if file.metadata()?.is_dir() { 2 } else { 1 };
    for name in names.into_iter().take(count) {
        // SAFETY: live fd, static NUL-terminated name; zero length only queries size.
        let size =
            unsafe { libc::fgetxattr(file.as_raw_fd(), name.as_ptr(), std::ptr::null_mut(), 0) };
        if size >= 0 {
            anyhow::bail!("Private-state POSIX ACL requires owner review");
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ENODATA) {
            return Err(error).context("inspect private-state POSIX ACL");
        }
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(super) fn check(_: &File) -> Result<()> {
    anyhow::bail!("Private Durable state ACL admission is unsupported on this platform")
}
