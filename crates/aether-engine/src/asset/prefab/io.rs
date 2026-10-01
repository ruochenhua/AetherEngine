use super::PrefabError;
use std::path::Path;

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), PrefabError> {
    use std::fs::{self, File, OpenOptions};
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| io_error(path, error))?;
    let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temp = parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), sequence));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| io_error(&temp, error))?;
        file.write_all(bytes)
            .map_err(|error| io_error(&temp, error))?;
        file.sync_all().map_err(|error| io_error(&temp, error))?;
        match fs::rename(&temp, path) {
            Ok(()) => Ok(()),
            Err(_error) if path.exists() => {
                let backup =
                    parent.join(format!(".{name}.{}.{}.bak", std::process::id(), sequence));
                fs::rename(path, &backup).map_err(|rename_error| io_error(path, rename_error))?;
                if let Err(rename_error) = fs::rename(&temp, path) {
                    let _ = fs::rename(&backup, path);
                    return Err(io_error(path, rename_error));
                }
                let _ = fs::remove_file(backup);
                Ok(())
            }
            Err(error) => Err(io_error(path, error)),
        }
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
    result
}

fn io_error(path: &Path, error: std::io::Error) -> PrefabError {
    PrefabError::Io {
        path: path.display().to_string(),
        message: error.to_string(),
    }
}
