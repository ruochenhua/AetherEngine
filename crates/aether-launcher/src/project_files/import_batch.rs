use super::{create_folder, import_file, FileOperationError};
use std::{fs, path::Path};

#[derive(Debug, Default)]
pub struct ImportBatchReport {
    pub imported: Vec<String>,
    pub failures: Vec<String>,
}

/// Import files and directories into a project folder, preserving directory structure.
/// Directories are copied under their source name; unsupported or invalid files are reported
/// individually so one bad item does not prevent the rest of the batch from importing.
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
                    import_directory(root, destination, source, &mut report)
                } else {
                    import_file(root, destination, source).map(|path| report.imported.push(path))
                }
            });
        if let Err(error) = result {
            report
                .failures
                .push(format!("{}: {error}", source.display()));
        }
    }
    report
}

fn import_directory(
    root: &Path,
    destination: &str,
    source: &Path,
    report: &mut ImportBatchReport,
) -> Result<(), FileOperationError> {
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
    let imported_directory = create_folder(root, destination, name)?;
    let mut children = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let child_path = child.path();
        let metadata = match fs::symlink_metadata(&child_path) {
            Ok(metadata) => metadata,
            Err(error) => {
                report
                    .failures
                    .push(format!("{}: {error}", child_path.display()));
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            report.failures.push(format!(
                "{}: Invalid path: symbolic links are not imported",
                child_path.display()
            ));
        } else if metadata.is_dir() {
            if let Err(error) = import_directory(root, &imported_directory, &child_path, report) {
                report
                    .failures
                    .push(format!("{}: {error}", child_path.display()));
            }
        } else {
            match import_file(root, &imported_directory, &child_path) {
                Ok(path) => report.imported.push(path),
                Err(error) => report
                    .failures
                    .push(format!("{}: {error}", child_path.display())),
            }
        }
    }
    Ok(())
}
