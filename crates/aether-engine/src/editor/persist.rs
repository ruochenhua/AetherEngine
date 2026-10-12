use super::error::EditorError;
use super::operation::{EditorContext, OperationLog};
use crate::scene::serializer;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

pub(super) fn persist(ctx: &EditorContext<'_>, log: &OperationLog) -> Result<(), EditorError> {
    persist_with(ctx, log, atomic_write)
}

fn persist_with<F>(
    ctx: &EditorContext<'_>,
    log: &OperationLog,
    mut write: F,
) -> Result<(), EditorError>
where
    F: FnMut(&Path, &[u8]) -> Result<(), EditorError>,
{
    let description = serializer::serialize_world(ctx.world, ctx.lighting, ctx.scene_name);
    let scene = serializer::to_ron_string(&description)
        .map_err(|error| EditorError::Serialization(error.to_string()))?;
    let operations = log.to_jsonl()?;
    let old_scene = read_optional(ctx.save_target)?;
    let old_operations = read_optional(ctx.operation_log_target)?;
    write(ctx.save_target, scene.as_bytes())?;
    if let Err(original) = write(ctx.operation_log_target, operations.as_bytes()) {
        let mut rollback_errors = Vec::new();
        if let Err(error) = restore_file_with(ctx.save_target, old_scene.as_deref(), &mut write) {
            rollback_errors.push(error.to_string());
        }
        if let Err(error) = restore_file_with(
            ctx.operation_log_target,
            old_operations.as_deref(),
            &mut write,
        ) {
            rollback_errors.push(error.to_string());
        }
        if !rollback_errors.is_empty() {
            return Err(EditorError::PersistenceRollbackFailed {
                original: original.to_string(),
                rollback: rollback_errors.join("; "),
            });
        }
        return Err(original);
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

fn restore_file_with<F>(path: &Path, bytes: Option<&[u8]>, write: &mut F) -> Result<(), EditorError>
where
    F: FnMut(&Path, &[u8]) -> Result<(), EditorError>,
{
    match bytes {
        Some(content) => write(path, content),
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
    let thread_name = thread.name().unwrap_or("editor").replace(':', "_");
    let suffix = format!("{}.{}.tmp", std::process::id(), thread_name);
    path.with_file_name(format!(
        ".{}.{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        suffix
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ecs::World, editor::EditorContext, renderer::light::LightingUniforms};
    use std::{
        io,
        sync::atomic::{AtomicUsize, Ordering},
        time::SystemTime,
    };

    fn injected_failure(path: &Path, message: &str) -> EditorError {
        EditorError::Io {
            path: path.display().to_string(),
            source: io::Error::other(message),
        }
    }

    #[test]
    fn second_file_and_first_file_restore_failures_report_both_errors() {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "aether-editor-persist-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let scene = root.join("scene.ron");
        let operations = root.join("scene.editor.jsonl");
        fs::write(&scene, "old scene bytes").unwrap();
        fs::write(&operations, "old operation bytes").unwrap();
        let mut world = World::new();
        let lighting = LightingUniforms::default();
        let context = EditorContext::new(&mut world, &lighting, "scene", &scene, &operations);
        let mut writes = 0;

        let error = persist_with(&context, &OperationLog::default(), |path, bytes| {
            writes += 1;
            match writes {
                1 => atomic_write(path, bytes),
                2 => Err(injected_failure(path, "operation log write failed")),
                3 => Err(injected_failure(path, "scene restore failed")),
                _ => atomic_write(path, bytes),
            }
        })
        .unwrap_err();

        let message = error.to_string();
        assert!(message.contains("operation log write failed"));
        assert!(message.contains("scene restore failed"));
        assert_eq!(
            fs::read_to_string(&operations).unwrap(),
            "old operation bytes"
        );
        assert_ne!(fs::read_to_string(&scene).unwrap(), "old scene bytes");
        fs::remove_dir_all(root).unwrap();
    }
}
