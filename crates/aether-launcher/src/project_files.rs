//! Validated project filesystem operations, independent of editor UI state.
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
#[path = "project_files/import_batch.rs"]
mod import_batch;
#[path = "project_files/obj_dependencies.rs"]
mod obj_dependencies;
#[path = "project_files/publish.rs"]
mod publish;
#[path = "project_files/validate.rs"]
mod validate;
pub use import_batch::{import_paths, ImportBatchReport};

const BYTE_LIMIT: u64 = 64 * 1024 * 1024;
static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn validate_obj_asset(
    root: &Path,
    relative_path: &Path,
) -> Result<PathBuf, FileOperationError> {
    obj_dependencies::validate_obj_asset(root, relative_path)
}

#[derive(Debug)]
pub enum FileOperationError {
    Path(String),
    Collision(String),
    UnsupportedFormat(String),
    ExternalDependency(String),
    Validation(String),
    Io(std::io::Error),
}
impl std::fmt::Display for FileOperationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Path(s) => write!(f, "Invalid path: {s}"),
            Self::Collision(s) => write!(f, "Target already exists: {s}"),
            Self::UnsupportedFormat(s) => write!(f, "Unsupported format: {s}"),
            Self::ExternalDependency(s) => write!(f, "External dependency: {s}"),
            Self::Validation(s) => write!(f, "Invalid asset: {s}"),
            Self::Io(e) => write!(f, "Filesystem error: {e}"),
        }
    }
}
impl std::error::Error for FileOperationError {}
impl From<std::io::Error> for FileOperationError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

/// Create one folder in an existing assets/ or scenes/ directory.
pub fn create_folder(
    root: &Path,
    directory: &str,
    name: &str,
) -> Result<String, FileOperationError> {
    valid_name(name)?;
    let destination = destination_path(root, directory, false)?;
    collision(&destination, name)?;
    fs::create_dir(destination.join(name)).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            FileOperationError::Collision(name.into())
        } else {
            error.into()
        }
    })?;
    Ok(format!("{directory}/{name}"))
}

/// Import one validated image, self-contained binary glTF, or text-based OBJ asset file,
/// without replacing targets. OBJ material libraries are imported as text dependencies.
pub fn import_file(
    root: &Path,
    directory: &str,
    source: &Path,
) -> Result<String, FileOperationError> {
    let destination = destination_path(root, directory, true)?;
    let metadata = fs::symlink_metadata(source)?;
    if !metadata.file_type().is_file() {
        return Err(FileOperationError::Path(
            "Source must be a regular non-symlink file".into(),
        ));
    }
    let name = source
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| FileOperationError::Path("Non-Unicode filename".into()))?;
    valid_name(name)?;
    collision(&destination, name)?;
    let extension = source
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(
        extension.as_str(),
        "png" | "jpg" | "jpeg" | "glb" | "obj" | "mtl"
    ) {
        return Err(FileOperationError::UnsupportedFormat(extension));
    }
    let mut bytes = Vec::new();
    fs::File::open(source)?
        .take(BYTE_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > BYTE_LIMIT {
        return Err(FileOperationError::Validation(
            "Asset exceeds 64 MiB byte budget".into(),
        ));
    }
    validate::bytes(&bytes, &extension)?;
    let (staging, mut file) = stage(&destination, &extension)?;
    (|| {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        let staged = fs::read(&staging.0)?;
        if staged != bytes {
            return Err(FileOperationError::Validation(
                "Staged bytes differ from source".into(),
            ));
        }
        validate_staged_asset(
            root,
            &staging.0,
            &format!("{directory}/{}", stage_name(&staging.0)),
        )?;
        // Revalidate after parsing: the UI may have changed the directory while loading.
        let checked = destination_path(root, directory, true)?;
        if checked != destination {
            return Err(FileOperationError::Path(
                "Destination changed during import".into(),
            ));
        }
        collision(&destination, name)?;
        publish::rename_exclusive(&staging.0, &destination.join(name))?;
        Ok(format!("{directory}/{name}"))
    })()
}

fn destination_path(
    root: &Path,
    directory: &str,
    assets_only: bool,
) -> Result<PathBuf, FileOperationError> {
    real_directory(root)?;
    if directory.contains('\\')
        || directory
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err(FileOperationError::Path(directory.into()));
    }
    let mut components = Path::new(directory).components();
    let Some(Component::Normal(first)) = components.next() else {
        return Err(FileOperationError::Path(directory.into()));
    };
    if first != "assets" && (assets_only || first != "scenes") {
        return Err(FileOperationError::Path(directory.into()));
    }
    let mut path = root.join(first);
    real_directory(&path)?;
    for component in components {
        let Component::Normal(name) = component else {
            return Err(FileOperationError::Path(directory.into()));
        };
        path.push(name);
        real_directory(&path)?;
    }
    Ok(path)
}
fn real_directory(path: &Path) -> Result<(), FileOperationError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|e| FileOperationError::Path(format!("{}: {e}", path.display())))?;
    if !metadata.file_type().is_dir() {
        return Err(FileOperationError::Path(format!(
            "{} is not a real directory",
            path.display()
        )));
    }
    Ok(())
}
fn valid_name(name: &str) -> Result<(), FileOperationError> {
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if name.trim().is_empty()
        || name.starts_with('.')
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c.is_control() || "\\/:*?\"<>|".contains(c))
        || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        || !matches!(
            Path::new(name).components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        )
    {
        return Err(FileOperationError::Path(format!(
            "Invalid filename: {name:?}"
        )));
    }
    Ok(())
}
fn collision(directory: &Path, name: &str) -> Result<(), FileOperationError> {
    for entry in fs::read_dir(directory)? {
        if entry?.file_name().to_string_lossy().to_lowercase() == name.to_lowercase() {
            return Err(FileOperationError::Collision(name.into()));
        }
    }
    Ok(())
}
struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn stage(directory: &Path, extension: &str) -> Result<(Staging, fs::File), FileOperationError> {
    loop {
        let path = directory.join(format!(
            ".aether-import-{}-{}.{}",
            std::process::id(),
            NEXT_STAGE.fetch_add(1, Ordering::Relaxed),
            extension
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => return Ok((Staging(path), file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
}

pub(super) fn supported_extension(extension: &str) -> bool {
    matches!(extension, "png" | "jpg" | "jpeg" | "glb" | "obj" | "mtl")
}

pub(super) fn validate_staged_asset(
    root: &Path,
    path: &Path,
    relative: &str,
) -> Result<(), FileOperationError> {
    let extension = Path::new(relative)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let bytes = read_bounded(path)?;
    validate::bytes(&bytes, &extension)?;
    match extension.as_str() {
        "glb" => validate::engine_mesh(path),
        "obj" => {
            obj_dependencies::validate_obj_asset(root, Path::new(relative))?;
            std::panic::catch_unwind(|| aether_engine::asset::loaders::obj::load(path))
                .map_err(|_| {
                    FileOperationError::Validation("OBJ loader rejected malformed mesh data".into())
                })?
                .map(|_| ())
                .map_err(|error| FileOperationError::Validation(error.to_string()))
        }
        _ => Ok(()),
    }
}

pub(super) fn read_bounded(path: &Path) -> Result<Vec<u8>, FileOperationError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > BYTE_LIMIT {
        return Err(FileOperationError::Validation(
            "Asset must be a regular file within the 64 MiB byte budget".into(),
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(path)?
        .take(BYTE_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > BYTE_LIMIT {
        return Err(FileOperationError::Validation(
            "Asset exceeds 64 MiB byte budget".into(),
        ));
    }
    Ok(bytes)
}

fn stage_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}
#[cfg(test)]
#[path = "project_files/tests.rs"]
mod tests;
