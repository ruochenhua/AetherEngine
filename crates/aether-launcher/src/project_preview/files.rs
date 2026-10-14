use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

/// Match the Project scanner's no-symlinks policy, including every ancestor.
/// Revalidate immediately before worker reads; canonical containment is a second check.
pub(super) fn contained(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err("Unsafe path or dependency outside project".into());
    }
    let mut ancestor = PathBuf::new();
    for component in root.components() {
        ancestor.push(component);
        if fs::symlink_metadata(&ancestor)
            .map_err(|e| format!("Unavailable project: {e}"))?
            .file_type()
            .is_symlink()
        {
            return Err("Project ancestor is a symlink".into());
        }
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|e| format!("Unavailable project: {e}"))?;
    let mut candidate = root.to_path_buf();
    for component in relative.components() {
        candidate.push(component);
        if fs::symlink_metadata(&candidate)
            .map_err(|e| format!("Unreadable resource or dependency: {e}"))?
            .file_type()
            .is_symlink()
        {
            return Err("Resource or dependency is a symlink".into());
        }
    }
    let canonical = candidate
        .canonicalize()
        .map_err(|e| format!("Unreadable resource: {e}"))?;
    if !canonical.starts_with(canonical_root) {
        return Err("Resource or dependency outside project".into());
    }
    if !canonical.is_file() {
        return Err("Resource or dependency is not a file".into());
    }
    Ok(canonical)
}

pub(super) fn dependency(root: &Path, asset: &Path, uri: &str) -> Result<PathBuf, String> {
    if uri.is_empty() || uri.contains([':', '%', '\\']) {
        return Err("Unsafe or external dependency URI".into());
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|e| format!("Unavailable project: {e}"))?;
    let parent = asset.parent().ok_or("Missing asset directory")?;
    let relative_parent = parent
        .strip_prefix(&canonical_root)
        .map_err(|_| "Asset dependency outside project")?;
    contained(root, &relative_parent.join(uri))
}

pub(super) fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|e| format!("Unreadable resource: {e}"))?;
    if file
        .metadata()
        .map_err(|e| format!("Unreadable resource: {e}"))?
        .len()
        > limit
    {
        return Err("Input file limit exceeded".into());
    }
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Unreadable resource: {e}"))?;
    if bytes.len() as u64 > limit {
        return Err("Input file limit exceeded".into());
    }
    Ok(bytes)
}
