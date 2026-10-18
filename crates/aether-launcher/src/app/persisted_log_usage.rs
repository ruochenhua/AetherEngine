//! Read-only resource inventory of the active scene's persisted operation log.
#![allow(dead_code)] // Staged for the owner-snapshot aggregator; no UI consumes this API yet.

use aether_engine::editor::{HistoryResourceUse, OperationLog};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PersistedLogState {
    NotApplicable,
    Missing,
    Complete,
    Unavailable(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PersistedLogResourceSnapshot {
    pub source_path: Option<PathBuf>,
    pub state: PersistedLogState,
    pub resources: Vec<HistoryResourceUse>,
}

pub(crate) fn capture_persisted_log_resource_usage(
    scene_path: Option<&Path>,
    project_root: &Path,
) -> PersistedLogResourceSnapshot {
    let Some(scene_path) = scene_path else {
        return snapshot(None, PersistedLogState::NotApplicable, Vec::new());
    };
    let source_path = super::editor::operation_log_path(scene_path);
    let root = match fs::canonicalize(project_root) {
        Ok(root) => root,
        Err(error) => return unavailable(source_path, "project root", error),
    };
    let resolved_scene = resolve_against_root(scene_path, &root);
    let canonical_scene = match fs::canonicalize(&resolved_scene) {
        Ok(scene) if scene.starts_with(&root) => scene,
        Ok(_) => {
            return unavailable_message(source_path, "active scene resolves outside project root");
        }
        Err(error) => return unavailable(source_path, "active scene", error),
    };
    let _ = canonical_scene;

    let log_target = resolve_against_root(&source_path, &root);
    let canonical_log = match fs::canonicalize(&log_target) {
        Ok(log) if log.starts_with(&root) => log,
        Ok(_) => {
            return unavailable_message(source_path, "operation log resolves outside project root");
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            match fs::symlink_metadata(&log_target) {
                Ok(_) => {
                    return unavailable_message(
                        source_path,
                        "operation log exists but its target cannot be canonicalized",
                    );
                }
                Err(metadata_error) if metadata_error.kind() != io::ErrorKind::NotFound => {
                    return unavailable(source_path, "operation log metadata", metadata_error);
                }
                Err(_) => {}
            }
            return match canonical_parent(&log_target) {
                Ok(parent) if parent.starts_with(&root) => {
                    snapshot(Some(source_path), PersistedLogState::Missing, Vec::new())
                }
                Ok(_) => {
                    unavailable_message(source_path, "operation log parent is outside project root")
                }
                Err(parent_error) => unavailable(source_path, "operation log parent", parent_error),
            };
        }
        Err(error) => return unavailable(source_path, "operation log", error),
    };

    let prevalidated_metadata = match fs::metadata(&canonical_log) {
        Ok(metadata) if metadata.file_type().is_file() => metadata,
        Ok(_) => {
            return unavailable_message(source_path, "operation log is not a regular file");
        }
        Err(error) => return unavailable(source_path, "operation log metadata", error),
    };
    let file = match open_contained_file(&root, &canonical_log, &prevalidated_metadata) {
        Ok(file) => file,
        Err(error) => return unavailable(source_path, "operation log secure open", error),
    };
    let mut content = String::new();
    if let Err(error) = (&file).read_to_string(&mut content) {
        return unavailable(source_path, "operation log read", error);
    };
    match OperationLog::from_jsonl(&content) {
        Ok(log) => snapshot(
            Some(source_path),
            PersistedLogState::Complete,
            log.resource_usage(&root),
        ),
        Err(error) => {
            unavailable_message(source_path, &format!("operation log parse failed: {error}"))
        }
    }
}

fn resolve_against_root(path: &Path, root: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn canonical_parent(path: &Path) -> io::Result<PathBuf> {
    path.parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))
        .and_then(fs::canonicalize)
}

fn snapshot(
    source_path: Option<PathBuf>,
    state: PersistedLogState,
    resources: Vec<HistoryResourceUse>,
) -> PersistedLogResourceSnapshot {
    PersistedLogResourceSnapshot {
        source_path,
        state,
        resources,
    }
}

fn unavailable(
    path: PathBuf,
    label: &str,
    error: impl std::fmt::Display,
) -> PersistedLogResourceSnapshot {
    unavailable_message(path, &format!("{label} unavailable: {error}"))
}

fn unavailable_message(path: PathBuf, reason: &str) -> PersistedLogResourceSnapshot {
    snapshot(
        Some(path),
        PersistedLogState::Unavailable(reason.to_owned()),
        Vec::new(),
    )
}

#[cfg(unix)]
fn open_rooted_regular_file(root: &Path, relative: &Path) -> io::Result<File> {
    use std::{
        ffi::CString,
        os::fd::{AsRawFd, FromRawFd},
        os::unix::ffi::OsStrExt,
    };

    let mut directory = open_root_directory(root)?;
    let mut components = relative.components().peekable();
    while let Some(component) = components.next() {
        let std::path::Component::Normal(name) = component else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "canonical log path is not a normal relative path",
            ));
        };
        let name = CString::new(name.as_bytes()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "path component contains NUL")
        })?;
        let is_final = components.peek().is_none();
        let flags = if is_final {
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC
        } else {
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC
        };
        // SAFETY: current directory descriptor is valid; `name` is a single
        // NUL-terminated component, and openat returns a new descriptor.
        let next_fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        if next_fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `next_fd` ownership transfers to this File exactly once.
        let next = unsafe { File::from_raw_fd(next_fd) };
        if is_final {
            regular_file_metadata(&next)?;
            return Ok(next);
        }
        directory = next;
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "canonical log path is empty",
    ))
}

#[cfg(unix)]
fn open_root_directory(root: &Path) -> io::Result<File> {
    use std::{
        ffi::CString,
        os::fd::{AsRawFd, FromRawFd},
        os::unix::ffi::OsStrExt,
    };

    if !root.has_root() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "project root is not absolute",
        ));
    }
    let slash = CString::new("/")
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid filesystem root"))?;
    // SAFETY: opening the filesystem root returns an owned descriptor.
    let fd = unsafe {
        libc::open(
            slash.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `fd` ownership transfers to this File exactly once.
    let mut directory = unsafe { File::from_raw_fd(fd) };
    for component in root.components() {
        let name = match component {
            std::path::Component::RootDir => continue,
            std::path::Component::Normal(name) => name,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "project root is not canonical",
                ));
            }
        };
        let name = CString::new(name.as_bytes()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "root component contains NUL")
        })?;
        // SAFETY: descriptor and one-component name are valid; openat returns a new descriptor.
        let next_fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if next_fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `next_fd` ownership transfers to this File exactly once.
        directory = unsafe { File::from_raw_fd(next_fd) };
    }
    Ok(directory)
}

#[cfg(unix)]
fn open_contained_file(
    root: &Path,
    canonical_log: &Path,
    expected: &fs::Metadata,
) -> io::Result<File> {
    let relative = canonical_log.strip_prefix(root).map_err(|_| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "operation log is outside project root",
        )
    })?;
    let file = open_rooted_regular_file(root, relative)?;
    let opened = regular_file_metadata(&file)?;
    validate_path_metadata_identity(expected, &opened)?;
    Ok(file)
}

#[cfg(windows)]
fn open_contained_file(
    root: &Path,
    canonical_log: &Path,
    _expected: &fs::Metadata,
) -> io::Result<File> {
    let prevalidated = open_log_file(canonical_log)?;
    regular_file_metadata(&prevalidated)?;
    ensure_handle_contained(&prevalidated, root)?;
    let opened = open_log_file(canonical_log)?;
    regular_file_metadata(&opened)?;
    validate_opened_file_identity(&prevalidated, &opened)?;
    ensure_handle_contained(&opened, root)?;
    Ok(opened)
}

#[cfg(not(any(unix, windows)))]
fn open_contained_file(
    _root: &Path,
    _canonical_log: &Path,
    _expected: &fs::Metadata,
) -> io::Result<File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "secure handle containment is unavailable on this platform",
    ))
}

fn open_log_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Do not follow a final-component reparse point if the canonical path races.
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options.open(path)
}

fn regular_file_metadata(file: &File) -> io::Result<fs::Metadata> {
    let metadata = file.metadata()?;
    if metadata.file_type().is_file() && !metadata.file_type().is_symlink() {
        Ok(metadata)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "opened operation log is not a regular file",
        ))
    }
}

fn validate_path_metadata_identity(
    path_metadata: &fs::Metadata,
    handle_metadata: &fs::Metadata,
) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if (path_metadata.dev(), path_metadata.ino())
            == (handle_metadata.dev(), handle_metadata.ino())
        {
            return Ok(());
        }
    }
    #[cfg(windows)]
    {
        // Windows std exposes stable timestamps but not stable file IDs. The
        // handle-to-handle check below uses GetFileInformationByHandle instead.
        let _ = (path_metadata, handle_metadata);
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "operation log target changed during open",
    ))
}

fn validate_opened_file_identity(prevalidated: &File, opened: &File) -> io::Result<()> {
    regular_file_metadata(prevalidated)?;
    regular_file_metadata(opened)?;
    if file_identity(prevalidated)? == file_identity(opened)? {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "opened operation log identity differs from the prevalidated file",
        ))
    }
}

#[cfg(windows)]
fn ensure_handle_contained(file: &File, canonical_root: &Path) -> io::Result<()> {
    use std::os::windows::{ffi::OsStringExt, io::AsRawHandle};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFinalPathNameByHandleW(
            handle: *mut std::ffi::c_void,
            path: *mut u16,
            path_length: u32,
            flags: u32,
        ) -> u32;
    }

    let handle = file.as_raw_handle();
    // SAFETY: null buffer with zero length is the documented size query.
    let required = unsafe { GetFinalPathNameByHandleW(handle, std::ptr::null_mut(), 0, 0) };
    if required == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut buffer = vec![0_u16; required as usize + 1];
    // SAFETY: buffer has more elements than the requested capacity and handle
    // remains valid for the duration of the call.
    let written =
        unsafe { GetFinalPathNameByHandleW(handle, buffer.as_mut_ptr(), buffer.len() as u32, 0) }
            as usize;
    if written == 0 || written >= buffer.len() {
        return Err(io::Error::last_os_error());
    }
    let final_path = PathBuf::from(std::ffi::OsString::from_wide(&buffer[..written]));
    let normalize = |path: &Path| {
        let path = path.to_string_lossy();
        let path = path
            .strip_prefix(r"\\?\UNC\")
            .map(|rest| format!(r"\\{rest}"))
            .or_else(|| path.strip_prefix(r"\\?\").map(str::to_owned))
            .unwrap_or_else(|| path.into_owned());
        path.to_lowercase()
    };
    let final_path = normalize(&final_path);
    let root = normalize(canonical_root);
    if Path::new(&final_path).starts_with(Path::new(&root)) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "opened operation log handle resolves outside project root",
        ))
    }
}

#[cfg(unix)]
#[derive(Debug, Eq, PartialEq)]
struct FileIdentity(u64, u64);

#[cfg(unix)]
fn file_identity(file: &File) -> io::Result<FileIdentity> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok(FileIdentity(metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
#[derive(Debug, Eq, PartialEq)]
struct FileIdentity(u32, u64);

#[cfg(windows)]
fn file_identity(file: &File) -> io::Result<FileIdentity> {
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    #[allow(dead_code)] // Unused ABI fields preserve the Win32 structure layout.
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[repr(C)]
    #[allow(dead_code)] // Unused ABI fields preserve the Win32 structure layout.
    struct ByHandleFileInformation {
        attributes: u32,
        creation_time: FileTime,
        last_access_time: FileTime,
        last_write_time: FileTime,
        volume_serial: u32,
        size_high: u32,
        size_low: u32,
        links: u32,
        index_high: u32,
        index_low: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(
            handle: *mut std::ffi::c_void,
            information: *mut ByHandleFileInformation,
        ) -> i32;
    }

    // SAFETY: the handle is owned by `file`, remains valid for this call, and
    // the output points to a correctly sized writable structure.
    let mut information: ByHandleFileInformation = unsafe { std::mem::zeroed() };
    let success = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) };
    if success == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(FileIdentity(
        information.volume_serial,
        (u64::from(information.index_high) << 32) | u64::from(information.index_low),
    ))
}

#[cfg(not(any(unix, windows)))]
#[derive(Debug, Eq, PartialEq)]
struct FileIdentity(u64, std::time::SystemTime, std::time::SystemTime);

#[cfg(not(any(unix, windows)))]
fn file_identity(file: &File) -> io::Result<FileIdentity> {
    fallback_metadata_identity(&file.metadata()?)
}

#[cfg(not(any(unix, windows)))]
fn fallback_metadata_identity(metadata: &fs::Metadata) -> io::Result<FileIdentity> {
    // Other targets have no stable identity API in std. Fail closed unless both
    // timestamps are available; compare length plus creation/modification time.
    Ok(FileIdentity(
        metadata.len(),
        metadata.created()?,
        metadata.modified()?,
    ))
}

#[cfg(test)]
#[path = "persisted_log_usage_tests.rs"]
mod tests;
