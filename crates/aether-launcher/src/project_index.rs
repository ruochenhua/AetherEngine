use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

mod classify;
use classify::classify;
const CONTENT_ROOTS: [&str; 2] = ["assets", "scenes"];

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ResourceKind {
    Scene,
    Prefab,
    Material,
    Gltf,
    Obj,
    Image,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceValidity {
    Valid,
    Invalid,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceEntry {
    /// Slash-separated path relative to the project root.
    pub path: String,
    pub kind: ResourceKind,
    pub validity: ResourceValidity,
    pub size: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IndexSnapshot {
    pub generation: u64,
    pub entries: Arc<[ResourceEntry]>,
    /// Names of explicitly allowlisted roots absent from this project.
    pub missing_roots: Arc<[String]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IndexError {
    ProjectRootSymlink(PathBuf),
    ContentRootSymlink(PathBuf),
    Scan(String),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum IndexStatus {
    #[default]
    Idle,
    Scanning,
    Ready,
    Error(String),
}

impl std::fmt::Display for IndexError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProjectRootSymlink(path) => {
                write!(formatter, "project root is a symlink: {}", path.display())
            }
            Self::ContentRootSymlink(path) => {
                write!(formatter, "content root is a symlink: {}", path.display())
            }
            Self::Scan(error) => formatter.write_str(error),
        }
    }
}

impl std::error::Error for IndexError {}

/// Read metadata and bounded RON headers from the two explicitly supported roots.
pub fn scan_project(root: &Path) -> Result<IndexSnapshot, IndexError> {
    let root_meta = fs::symlink_metadata(root).map_err(scan_io)?;
    if root_meta.file_type().is_symlink() {
        return Err(IndexError::ProjectRootSymlink(root.to_path_buf()));
    }
    if !root_meta.is_dir() {
        return Err(IndexError::Scan(format!(
            "project root is not a directory: {}",
            root.display()
        )));
    }
    let canonical_root = root.canonicalize().map_err(scan_io)?;
    let mut entries = Vec::new();
    let mut missing_roots = Vec::new();
    for name in CONTENT_ROOTS {
        let content_root = root.join(name);
        let metadata = match fs::symlink_metadata(&content_root) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                missing_roots.push(name.to_owned());
                continue;
            }
            Err(error) => return Err(scan_io(error)),
        };
        if metadata.file_type().is_symlink() {
            return Err(IndexError::ContentRootSymlink(content_root));
        }
        if !metadata.is_dir() {
            return Err(IndexError::Scan(format!(
                "content root is not a directory: {}",
                content_root.display()
            )));
        }
        let canonical_content = content_root.canonicalize().map_err(scan_io)?;
        if !canonical_content.starts_with(&canonical_root) {
            return Err(IndexError::Scan(format!(
                "content root escapes project: {}",
                content_root.display()
            )));
        }
        walk_content(
            root,
            &canonical_root,
            &canonical_content,
            &content_root,
            name,
            &mut entries,
        )?;
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    missing_roots.sort();
    Ok(IndexSnapshot {
        generation: 0,
        entries: entries.into(),
        missing_roots: missing_roots.into(),
    })
}

fn scan_io(error: io::Error) -> IndexError {
    IndexError::Scan(error.to_string())
}

fn walk_content(
    project_root: &Path,
    canonical_project: &Path,
    canonical_content: &Path,
    directory: &Path,
    root_name: &str,
    entries: &mut Vec<ResourceEntry>,
) -> Result<(), IndexError> {
    let mut children = fs::read_dir(directory)
        .map_err(scan_io)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(scan_io)?;
    children.sort_by_key(|item| item.file_name());
    for child in children {
        let path = child.path();
        let file_name = child.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if is_hidden_or_ignored(file_name)
            || (file_name.eq_ignore_ascii_case("reports")
                && directory.file_name().is_some_and(|name| name == "tests"))
        {
            continue;
        }
        let metadata = fs::symlink_metadata(&path).map_err(scan_io)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            let canonical = path.canonicalize().map_err(scan_io)?;
            if !canonical.starts_with(canonical_project)
                || !canonical.starts_with(canonical_content)
            {
                continue;
            }
            if !is_lexically_beneath(project_root, &path) {
                continue;
            }
            walk_content(
                project_root,
                canonical_project,
                canonical_content,
                &path,
                root_name,
                entries,
            )?;
        } else if metadata.is_file() {
            let canonical = path.canonicalize().map_err(scan_io)?;
            if !canonical.starts_with(canonical_project)
                || !canonical.starts_with(canonical_content)
            {
                continue;
            }
            if !is_lexically_beneath(project_root, &path) {
                continue;
            }
            let relative = path
                .strip_prefix(project_root)
                .map_err(|error| IndexError::Scan(error.to_string()))?;
            let normalized = relative
                .components()
                .filter_map(|part| match part {
                    Component::Normal(value) => value.to_str(),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("/");
            if let Some((kind, validity)) = classify(&path, root_name, metadata.len()) {
                entries.push(ResourceEntry {
                    path: normalized,
                    kind,
                    validity,
                    size: metadata.len(),
                });
            }
        }
    }
    Ok(())
}

fn is_lexically_beneath(root: &Path, candidate: &Path) -> bool {
    candidate.strip_prefix(root).is_ok_and(|relative| {
        relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    })
}

fn is_hidden_or_ignored(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name.to_ascii_lowercase().as_str(),
            "target"
                | "cache"
                | ".cache"
                | "output"
                | "outputs"
                | "build"
                | "dist"
                | "node_modules"
        )
}

#[derive(Default)]
pub struct ProjectIndex {
    next: u64,
    snapshot: IndexSnapshot,
    status: IndexStatus,
}

impl ProjectIndex {
    pub fn begin_scan(&mut self) -> u64 {
        self.next = self.next.saturating_add(1);
        self.status = IndexStatus::Scanning;
        self.next
    }
    pub fn publish(&mut self, generation: u64, result: Result<IndexSnapshot, IndexError>) {
        if generation != self.next {
            return;
        }
        match result {
            Ok(mut snapshot) => {
                snapshot.generation = generation;
                self.snapshot = snapshot;
                self.status = IndexStatus::Ready;
            }
            Err(error) => self.status = IndexStatus::Error(error.to_string()),
        }
    }
    pub fn snapshot(&self) -> &IndexSnapshot {
        &self.snapshot
    }
    pub fn status(&self) -> &IndexStatus {
        &self.status
    }
}

#[derive(Clone, Debug, Default)]
pub struct ProjectQuery {
    search: String,
    kind: Option<ResourceKind>,
    sort: ProjectSort,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProjectSort {
    #[default]
    Name,
    Path,
    Kind,
}

impl ProjectQuery {
    pub fn search(mut self, text: &str) -> Self {
        self.search = text.trim().to_lowercase();
        self
    }
    pub fn kind(mut self, kind: ResourceKind) -> Self {
        self.kind = Some(kind);
        self
    }
    pub fn sort(mut self, sort: ProjectSort) -> Self {
        self.sort = sort;
        self
    }
    pub fn apply<'a>(&self, snapshot: &'a IndexSnapshot) -> Vec<&'a ResourceEntry> {
        let mut result: Vec<_> = snapshot
            .entries
            .iter()
            .filter(|entry| {
                let matches_search =
                    self.search.is_empty() || entry.path.to_lowercase().contains(&self.search);
                let matches_kind = match self.kind {
                    Some(kind) => entry.kind == kind,
                    None => true,
                };
                matches_search && matches_kind
            })
            .collect();
        result.sort_by(|left, right| match self.sort {
            ProjectSort::Name => file_name(&left.path)
                .cmp(file_name(&right.path))
                .then_with(|| left.path.cmp(&right.path)),
            ProjectSort::Path => left.path.cmp(&right.path),
            ProjectSort::Kind => left
                .kind
                .cmp(&right.kind)
                .then_with(|| left.path.cmp(&right.path)),
        });
        result
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[derive(Default)]
pub struct PathHistory {
    paths: Vec<String>,
    cursor: usize,
}

impl PathHistory {
    pub fn visit(&mut self, path: &str) {
        if self
            .paths
            .get(self.cursor)
            .is_some_and(|current| current == path)
        {
            return;
        }
        self.paths.truncate(self.cursor.saturating_add(1));
        self.paths.push(path.to_owned());
        self.cursor = self.paths.len().saturating_sub(1);
    }
    pub fn back(&mut self) -> Option<String> {
        if self.cursor == 0 || self.paths.is_empty() {
            return None;
        }
        self.cursor -= 1;
        self.paths.get(self.cursor).cloned()
    }
    pub fn forward(&mut self) -> Option<String> {
        if self.cursor + 1 >= self.paths.len() {
            return None;
        }
        self.cursor += 1;
        self.paths.get(self.cursor).cloned()
    }
    pub fn current(&self) -> Option<&str> {
        self.paths.get(self.cursor).map(String::as_str)
    }
}

#[cfg(test)]
#[path = "project_index/tests.rs"]
mod tests;
