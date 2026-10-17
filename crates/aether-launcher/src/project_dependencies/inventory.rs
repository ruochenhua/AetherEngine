use super::{AnalysisBudget, Diagnostic, Fingerprint};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, Metadata},
    io::{self, Read},
    path::Path,
};

#[derive(Default)]
pub(super) struct Inventory {
    pub files: BTreeMap<String, Fingerprint>,
    pub directories: Vec<String>,
    pub documents: BTreeMap<String, Vec<u8>>,
    pub diagnostics: Vec<Diagnostic>,
    pub fingerprint: Fingerprint,
    pub stale: bool,
    entries: BTreeMap<String, u8>,
    total_bytes: u64,
    file_count: usize,
}
impl Inventory {
    pub fn diagnostic(&mut self, path: &str, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            path: path.into(),
            message: message.into(),
        });
    }
}

pub(super) fn scan(
    root: &Path,
    budget: AnalysisBudget,
    capture: bool,
) -> Result<Inventory, String> {
    let metadata = fs::symlink_metadata(root).map_err(|e| e.to_string())?;
    if metadata.is_symlink() || !metadata.is_dir() {
        return Err("Project root must be a directory, not a symlink".into());
    }
    let canonical = root.canonicalize().map_err(|e| e.to_string())?;
    let mut result = Inventory::default();
    for name in ["assets", "scenes"] {
        visit(root, &canonical, name, budget, capture, 0, &mut result);
    }
    let mut aggregate = Sha256::new();
    for (path, kind) in &result.entries {
        aggregate.update([*kind]);
        aggregate.update((path.len() as u64).to_le_bytes());
        aggregate.update(path.as_bytes());
        if let Some(hash) = result.files.get(path) {
            aggregate.update(hash);
        }
    }
    // Failed scans must never compare equal to a complete inventory.
    result.diagnostics.sort();
    for diagnostic in &result.diagnostics {
        aggregate.update(b"diagnostic");
        aggregate.update((diagnostic.path.len() as u64).to_le_bytes());
        aggregate.update(diagnostic.path.as_bytes());
        aggregate.update((diagnostic.message.len() as u64).to_le_bytes());
        aggregate.update(diagnostic.message.as_bytes());
    }
    result.fingerprint = aggregate.finalize().into();
    result.directories.sort();
    Ok(result)
}

fn visit(
    root: &Path,
    canonical: &Path,
    relative: &str,
    budget: AnalysisBudget,
    capture: bool,
    depth: usize,
    result: &mut Inventory,
) {
    if result.entries.len() >= budget.max_entries {
        result.diagnostic(relative, "Inventory entry count budget exceeded");
        return;
    }
    let path = root.join(relative);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound && depth == 0 => {
            result.entries.insert(relative.into(), b'-');
            return;
        }
        Err(e) => {
            result.diagnostic(relative, format!("Unreadable inventory entry: {e}"));
            result.stale |= e.kind() == io::ErrorKind::NotFound;
            return;
        }
    };
    if metadata.is_symlink() {
        result.entries.insert(relative.into(), b'l');
        result.diagnostic(relative, "Symlink entries are outside the dependency model");
        return;
    }
    if !path.canonicalize().is_ok_and(|p| p.starts_with(canonical)) {
        result.diagnostic(
            relative,
            "Inventory entry escapes the canonical project root",
        );
        return;
    }
    if depth == 0 && !metadata.is_dir() {
        result.entries.insert(relative.into(), b'!');
        result.diagnostic(relative, "Allowlisted content root must be a directory");
        return;
    }
    if metadata.is_dir() {
        result.entries.insert(relative.into(), b'd');
        result.directories.push(relative.into());
        if depth >= 128 {
            result.diagnostic(relative, "Directory depth budget exceeded");
            return;
        }
        let children = match fs::read_dir(&path) {
            Ok(children) => children,
            Err(e) => {
                result.diagnostic(relative, format!("Unreadable directory: {e}"));
                return;
            }
        };
        let remaining = budget.max_entries.saturating_sub(result.entries.len());
        let mut names = Vec::new();
        for child in children.take(remaining.saturating_add(1)) {
            match child {
                Ok(child) => match child.file_name().into_string() {
                    Ok(name) => names.push(name),
                    Err(_) => {
                        result.diagnostic(relative, "Non-UTF-8 inventory name cannot be normalized")
                    }
                },
                Err(e) => result.diagnostic(relative, format!("Unreadable directory entry: {e}")),
            }
        }
        if names.len() > remaining {
            result.diagnostic(relative, "Inventory entry count budget exceeded");
            return;
        }
        names.sort();
        for name in names {
            visit(
                root,
                canonical,
                &format!("{relative}/{name}"),
                budget,
                capture,
                depth + 1,
                result,
            );
        }
    } else if metadata.is_file() {
        result.entries.insert(relative.into(), b'f');
        result.file_count += 1;
        if result.file_count > budget.max_files {
            result.diagnostic(relative, "File count budget exceeded");
            return;
        }
        if metadata.len() > budget.max_file_bytes {
            result.diagnostic(relative, "Per-file byte budget exceeded");
            return;
        }
        if metadata.len() > budget.max_total_bytes.saturating_sub(result.total_bytes) {
            result.diagnostic(relative, "Aggregate byte budget exceeded");
            return;
        }
        match hash_file(&path, relative, &metadata, budget, capture, result) {
            Ok((hash, bytes)) => {
                result.files.insert(relative.into(), hash);
                if let Some(bytes) = bytes {
                    result.documents.insert(relative.into(), bytes);
                }
            }
            Err(e) => result.diagnostic(relative, e),
        }
    } else {
        result.diagnostic(relative, "Unsupported inventory entry type");
    }
}

fn hash_file(
    path: &Path,
    relative: &str,
    before: &Metadata,
    budget: AnalysisBudget,
    capture: bool,
    result: &mut Inventory,
) -> Result<(Fingerprint, Option<Vec<u8>>), String> {
    let mut file = File::open(path).map_err(|e| format!("Unreadable file: {e}"))?;
    let opened = file.metadata().map_err(|e| e.to_string())?;
    if !unchanged(before, &opened) {
        result.stale = true;
        return Err("File changed before reading".into());
    }
    let extension = extension(relative);
    let mut bytes =
        (capture && matches!(extension.as_str(), "ron" | "gltf" | "glb")).then(Vec::new);
    let mut digest = Sha256::new();
    digest.update((relative.len() as u64).to_le_bytes());
    digest.update(relative.as_bytes());
    let mut buffer = [0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let allowance = budget
            .max_file_bytes
            .saturating_sub(size)
            .min(budget.max_total_bytes.saturating_sub(result.total_bytes));
        let length = (buffer.len() as u64).min(allowance.saturating_add(1)) as usize;
        let count = file
            .read(&mut buffer[..length])
            .map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        if count as u64 > allowance {
            result.stale = true;
            return Err("Stream byte budget exceeded; file changed while reading".into());
        }
        size += count as u64;
        result.total_bytes += count as u64;
        digest.update(&buffer[..count]);
        if let Some(bytes) = &mut bytes {
            bytes.extend_from_slice(&buffer[..count]);
        }
    }
    let after = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if after.is_symlink() || !unchanged(before, &after) || size != before.len() {
        result.stale = true;
        return Err("File changed while reading".into());
    }
    Ok((digest.finalize().into(), bytes))
}
fn unchanged(a: &Metadata, b: &Metadata) -> bool {
    let basic = a.len() == b.len() && a.modified().ok() == b.modified().ok();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        basic
            && a.dev() == b.dev()
            && a.ino() == b.ino()
            && a.ctime() == b.ctime()
            && a.ctime_nsec() == b.ctime_nsec()
    }
    #[cfg(not(unix))]
    {
        basic
    }
}
pub(super) fn extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}
