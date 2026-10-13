//! Release-time resource validation shared by live material drops and replay.
use crate::project_index::ResourceKind;
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub(super) fn validate_resource(
    path: &Path,
    root: &Path,
    kind: ResourceKind,
) -> anyhow::Result<PathBuf> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let supported = match kind {
        ResourceKind::Material => extension == "ron",
        ResourceKind::Image => matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "hdr"),
        _ => false,
    };
    anyhow::ensure!(
        supported,
        "resource kind and extension do not match the drop target"
    );
    anyhow::ensure!(
        !path.is_absolute()
            && path.components().next() == Some(Component::Normal("assets".as_ref())),
        "resource must be project-relative under assets/"
    );
    anyhow::ensure!(
        !path
            .components()
            .any(|c| !matches!(c, Component::Normal(_))),
        "resource path cannot contain traversal components"
    );
    let metadata = fs::symlink_metadata(root)?;
    anyhow::ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "project root cannot be a symbolic link"
    );
    let canonical_root = root.canonicalize()?;
    let mut cursor = canonical_root.clone();
    for c in path.components() {
        cursor.push(c.as_os_str());
        anyhow::ensure!(
            !fs::symlink_metadata(&cursor)?.file_type().is_symlink(),
            "symbolic links are not supported for dropped resources"
        );
    }
    let canonical = cursor.canonicalize()?;
    anyhow::ensure!(
        canonical.starts_with(canonical_root.join("assets")) && fs::metadata(&canonical)?.is_file(),
        "resource must be a regular file within project assets/"
    );
    let max_bytes = if kind == ResourceKind::Material {
        256 * 1024
    } else {
        64 * 1024 * 1024
    };
    anyhow::ensure!(
        fs::metadata(&canonical)?.len() <= max_bytes,
        "resource exceeds the size limit ({max_bytes} bytes)"
    );
    Ok(canonical)
}

/// Material document references may use `..` within assets/, but may not traverse links.
pub(super) fn material_dependency(
    parent: &Path,
    path: &Path,
    root: &Path,
) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(
        !path.is_absolute(),
        "material texture reference must be relative to its document"
    );
    let canonical_root = root.canonicalize()?;
    let mut cursor = canonical_root.clone();
    for component in parent.join(path).components() {
        match component {
            Component::Normal(name) => {
                cursor.push(name);
                anyhow::ensure!(
                    !fs::symlink_metadata(&cursor)?.file_type().is_symlink(),
                    "material dependency cannot traverse symbolic links"
                );
            }
            Component::CurDir => {}
            Component::ParentDir => {
                anyhow::ensure!(
                    cursor != canonical_root,
                    "material dependency escapes the project"
                );
                cursor.pop();
            }
            _ => anyhow::bail!("invalid material dependency path"),
        }
    }
    anyhow::ensure!(
        cursor.starts_with(canonical_root.join("assets")),
        "material dependency must remain within assets/"
    );
    Ok(cursor.strip_prefix(canonical_root)?.to_path_buf())
}
