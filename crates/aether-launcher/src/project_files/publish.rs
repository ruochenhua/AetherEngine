//! Exclusive atomic publication. std::fs::rename may silently overwrite on Unix.
use super::FileOperationError;
use std::{io, path::Path};

pub(super) fn rename_exclusive(from: &Path, to: &Path) -> Result<(), FileOperationError> {
    rename(from, to).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            FileOperationError::Collision(to.display().to_string())
        } else {
            FileOperationError::Io(error)
        }
    })
}
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn rename(from: &Path, to: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let from = CString::new(from.as_os_str().as_bytes())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let to = CString::new(to.as_os_str().as_bytes())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    #[cfg(target_os = "macos")]
    let result = {
        unsafe extern "C" {
            fn renamex_np(
                from: *const std::ffi::c_char,
                to: *const std::ffi::c_char,
                flags: u32,
            ) -> i32;
        }
        // SAFETY: both C strings live through this call. RENAME_EXCL is 0x4 on Darwin.
        unsafe { renamex_np(from.as_ptr(), to.as_ptr(), 0x4) }
    };
    #[cfg(target_os = "linux")]
    let result = {
        unsafe extern "C" {
            fn renameat2(
                oldfd: i32,
                from: *const std::ffi::c_char,
                newfd: i32,
                to: *const std::ffi::c_char,
                flags: u32,
            ) -> i32;
        }
        // SAFETY: valid C strings; AT_FDCWD=-100, RENAME_NOREPLACE=1 on Linux.
        unsafe { renameat2(-100, from.as_ptr(), -100, to.as_ptr(), 1) }
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
#[cfg(windows)]
fn rename(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::rename(from, to)
}
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn rename(_from: &Path, _to: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Exclusive atomic rename is unavailable on this platform",
    ))
}
