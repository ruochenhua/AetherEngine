use super::error::EditorError;
use super::operation::{EditorContext, OperationLog};
use crate::scene::serializer;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

pub(super) fn persist(ctx: &EditorContext<'_>, log: &OperationLog) -> Result<(), EditorError> {
    let description = serializer::serialize_world(ctx.world, ctx.lighting, ctx.scene_name);
    let scene = serializer::to_ron_string(&description)
        .map_err(|error| EditorError::Serialization(error.to_string()))?;
    let operations = log.to_jsonl()?;
    let old_scene = read_optional(ctx.save_target)?;
    let old_operations = read_optional(ctx.operation_log_target)?;
    atomic_write(ctx.save_target, scene.as_bytes())?;
    if let Err(error) = atomic_write(ctx.operation_log_target, operations.as_bytes()) {
        restore_file(ctx.save_target, old_scene.as_deref())?;
        restore_file(ctx.operation_log_target, old_operations.as_deref())?;
        return Err(error);
    }
    Ok(())
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, EditorError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(EditorError::Io {
            path: path.display().to_string(),
            source,
        }),
    }
}

fn restore_file(path: &Path, bytes: Option<&[u8]>) -> Result<(), EditorError> {
    match bytes {
        Some(content) => atomic_write(path, content),
        None => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(EditorError::Io {
                path: path.display().to_string(),
                source,
            }),
        },
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), EditorError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let temp = temp_path(path);
    let result = (|| {
        let mut file = File::create(&temp).map_err(|source| EditorError::Io {
            path: temp.display().to_string(),
            source,
        })?;
        file.write_all(bytes).map_err(|source| EditorError::Io {
            path: temp.display().to_string(),
            source,
        })?;
        file.sync_all().map_err(|source| EditorError::Io {
            path: temp.display().to_string(),
            source,
        })?;
        fs::rename(&temp, path).map_err(|source| EditorError::Io {
            path: path.display().to_string(),
            source,
        })?;
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn temp_path(path: &Path) -> PathBuf {
    let thread = std::thread::current();
    let thread_name = thread.name().unwrap_or("editor");
    let suffix = format!("{}.{}.tmp", std::process::id(), thread_name);
    path.with_file_name(format!(
        ".{}.{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        suffix
    ))
}
