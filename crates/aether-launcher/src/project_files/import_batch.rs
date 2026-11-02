use super::{
    collision, destination_path, publish, read_bounded, supported_extension, validate_staged_asset,
    FileOperationError, BYTE_LIMIT, NEXT_STAGE,
};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};

const MAX_PACKAGE_DEPTH: usize = 32;
const MAX_PACKAGE_ENTRIES: usize = 4096;
const MAX_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Default)]
pub struct ImportBatchReport {
    pub imported: Vec<String>,
    pub failures: Vec<String>,
}

#[derive(Default)]
struct PackageManifest {
    directories: Vec<PathBuf>,
    files: Vec<PackageFile>,
    names: BTreeSet<String>,
    total_bytes: u64,
    entries: usize,
}

struct PackageFile {
    source: PathBuf,
    relative: PathBuf,
    extension: String,
}

/// Import independent top-level sources separately. A selected folder is a single
/// transaction: all supported files and dependencies publish together or not at all.
pub fn import_paths(
    root: &Path,
    destination: &str,
    sources: &[impl AsRef<Path>],
) -> ImportBatchReport {
    let mut report = ImportBatchReport::default();
    for source in sources {
        let source = source.as_ref();
        let result = fs::symlink_metadata(source)
            .map_err(FileOperationError::from)
            .and_then(|metadata| {
                if metadata.file_type().is_symlink() {
                    return Err(FileOperationError::Path(
                        "Source must not be a symbolic link".into(),
                    ));
                }
                if metadata.is_dir() {
                    import_directory(root, destination, source)
                } else {
                    super::import_file(root, destination, source).map(|path| vec![path])
                }
            });
        match result {
            Ok(imported) => report.imported.extend(imported),
            Err(error) => report
                .failures
                .push(format!("{}: {error}", source.display())),
        }
    }
    report
}

fn import_directory(
    root: &Path,
    destination: &str,
    source: &Path,
) -> Result<Vec<String>, FileOperationError> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(FileOperationError::Path(
            "Source must be a real directory".into(),
        ));
    }
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| FileOperationError::Path("Non-Unicode folder name".into()))?;
    super::valid_name(name)?;
    let destination_dir = destination_path(root, destination, true)?;
    collision(&destination_dir, name)?;

    let mut manifest = PackageManifest::default();
    collect_package(source, Path::new(""), 0, &mut manifest)?;
    let staging = stage_directory(&destination_dir)?;
    let staged_name = staging
        .0
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| FileOperationError::Path("Invalid staging directory name".into()))?;
    let staged_relative = format!("{destination}/{staged_name}");

    for directory in &manifest.directories {
        let target = staging.0.join(directory);
        fs::create_dir_all(&target)?;
    }
    for file in &manifest.files {
        let current = fs::symlink_metadata(&file.source)?;
        if current.file_type().is_symlink() || !current.is_file() || current.len() > BYTE_LIMIT {
            return Err(FileOperationError::Path(format!(
                "Source changed or is not a regular file: {}",
                file.relative.display()
            )));
        }
        let bytes = read_bounded(&file.source)?;
        super::validate::bytes(&bytes, &file.extension).map_err(|error| {
            FileOperationError::Validation(format!("{}: {error}", file.relative.display()))
        })?;
        let target = staging.0.join(&file.relative);
        let mut staged = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        staged.write_all(&bytes)?;
        staged.sync_all()?;
        drop(staged);
        if read_bounded(&target)? != bytes {
            return Err(FileOperationError::Validation(format!(
                "Staged bytes differ from source: {}",
                file.relative.display()
            )));
        }
    }

    for file in &manifest.files {
        let target = staging.0.join(&file.relative);
        let relative = format!("{staged_relative}/{}", slash_path(&file.relative));
        validate_staged_asset(root, &target, &relative).map_err(|error| {
            FileOperationError::Validation(format!("{}: {error}", file.relative.display()))
        })?;
    }

    let checked = destination_path(root, destination, true)?;
    if checked != destination_dir {
        return Err(FileOperationError::Path(
            "Destination changed during import".into(),
        ));
    }
    collision(&destination_dir, name)?;
    publish::rename_exclusive(&staging.0, &destination_dir.join(name))?;

    Ok(manifest
        .files
        .iter()
        .map(|file| format!("{destination}/{name}/{}", slash_path(&file.relative)))
        .collect())
}

fn collect_package(
    directory: &Path,
    relative: &Path,
    depth: usize,
    manifest: &mut PackageManifest,
) -> Result<(), FileOperationError> {
    if depth > MAX_PACKAGE_DEPTH {
        return Err(FileOperationError::Validation(format!(
            "Package directory depth exceeds {MAX_PACKAGE_DEPTH}"
        )));
    }
    let mut children = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let name = child
            .file_name()
            .into_string()
            .map_err(|_| FileOperationError::Path("Non-Unicode package entry".into()))?;
        if ignored_metadata(&name) {
            continue;
        }
        super::valid_name(&name)?;
        manifest.entries += 1;
        if manifest.entries > MAX_PACKAGE_ENTRIES {
            return Err(FileOperationError::Validation(format!(
                "Package exceeds {MAX_PACKAGE_ENTRIES} entries"
            )));
        }
        let child_relative = relative.join(&name);
        let normalized = slash_path(&child_relative).to_lowercase();
        if !manifest.names.insert(normalized.clone()) {
            return Err(FileOperationError::Collision(normalized));
        }
        let metadata = fs::symlink_metadata(child.path())?;
        if metadata.file_type().is_symlink() {
            return Err(FileOperationError::Path(format!(
                "Symbolic links are not imported: {}",
                child_relative.display()
            )));
        }
        if metadata.is_dir() {
            manifest.directories.push(child_relative.clone());
            collect_package(&child.path(), &child_relative, depth + 1, manifest)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(FileOperationError::Path(format!(
                "Unsupported package entry: {}",
                child_relative.display()
            )));
        }
        let extension = Path::new(&name)
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !supported_extension(&extension) {
            return Err(FileOperationError::UnsupportedFormat(format!(
                "{} ({extension})",
                child_relative.display()
            )));
        }
        if metadata.len() > BYTE_LIMIT {
            return Err(FileOperationError::Validation(format!(
                "Package file exceeds 64 MiB: {}",
                child_relative.display()
            )));
        }
        manifest.total_bytes = manifest
            .total_bytes
            .checked_add(metadata.len())
            .ok_or_else(|| FileOperationError::Validation("Package byte size overflow".into()))?;
        if manifest.total_bytes > MAX_PACKAGE_BYTES {
            return Err(FileOperationError::Validation(format!(
                "Package exceeds {} MiB",
                MAX_PACKAGE_BYTES / (1024 * 1024)
            )));
        }
        manifest.files.push(PackageFile {
            source: child.path(),
            relative: child_relative,
            extension,
        });
    }
    Ok(())
}

fn ignored_metadata(name: &str) -> bool {
    matches!(
        name,
        ".DS_Store" | ".localized" | "Thumbs.db" | "desktop.ini"
    ) || name.starts_with("._")
}

fn slash_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

struct StagingDirectory(PathBuf);
impl Drop for StagingDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn stage_directory(destination: &Path) -> Result<StagingDirectory, FileOperationError> {
    loop {
        let path = destination.join(format!(
            ".aether-import-{}-{}",
            std::process::id(),
            NEXT_STAGE.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(StagingDirectory(path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
}
