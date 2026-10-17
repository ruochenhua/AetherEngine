use super::{visit::AuthoredReference, ReferenceKind, Resolution};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub(super) fn candidate(source: &str, reference: &AuthoredReference) -> String {
    let base = if reference.material_relative {
        Path::new(source).parent().unwrap_or_else(|| Path::new(""))
    } else {
        Path::new("")
    };
    let path = base.join(&reference.path);
    if reference.material_relative || !material_texture(reference.kind) {
        if let Ok(path) = aether_engine::asset::CanonicalPath::new(Path::new("."), &path) {
            return path.as_str().into();
        }
        return normalize(Path::new(&path.to_string_lossy().replace('\\', "/")));
    }
    normalize(&path)
}
fn normalize(path: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for part in path.components() {
        match part {
            Component::CurDir => (),
            Component::ParentDir => {
                if parts.last().is_some_and(|p| p != ".." && p != "/") {
                    parts.pop();
                } else {
                    parts.push("..".into());
                }
            }
            Component::RootDir => parts.push("/".into()),
            Component::Prefix(prefix) => parts.push(prefix.as_os_str().to_string_lossy().into()),
            Component::Normal(value) => parts.push(value.to_string_lossy().into()),
        }
    }
    let normalized = parts.join("/");
    if normalized.starts_with("//") {
        normalized[1..].into()
    } else {
        normalized
    }
}

pub(super) fn resolve(root: &Path, candidate: &str) -> Resolution {
    let path = Path::new(candidate);
    let mut components = path.components();
    if !matches!(components.next(),Some(Component::Normal(name)) if name == "assets" || name == "scenes")
        || !components.all(|part| matches!(part, Component::Normal(_)))
    {
        return Resolution::Unsafe("Candidate is outside allowlisted content roots".into());
    }
    let mut prefix = PathBuf::from(root);
    let canonical = match root.canonicalize() {
        Ok(root) => root,
        Err(e) => return Resolution::Unsafe(e.to_string()),
    };
    for component in path.components() {
        prefix.push(component.as_os_str());
        match fs::symlink_metadata(&prefix) {
            Ok(metadata) if metadata.is_symlink() => {
                return Resolution::Unsafe("Candidate traverses a symlink".into())
            }
            Ok(_) => {
                if !prefix
                    .canonicalize()
                    .is_ok_and(|p| p.starts_with(&canonical))
                {
                    return Resolution::Unsafe(
                        "Candidate escapes the canonical project root".into(),
                    );
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Resolution::Missing
            }
            Err(error) => return Resolution::Unsafe(format!("Cannot inspect candidate: {error}")),
        }
    }
    if prefix.is_file() {
        Resolution::Resolved
    } else {
        Resolution::Unsafe("Candidate is not a regular file".into())
    }
}

pub(super) fn reference_resolution(
    root: &Path,
    reference: &AuthoredReference,
    candidate: &str,
) -> Resolution {
    if cfg!(unix) && reference.path.contains('\\') && material_texture(reference.kind) {
        return Resolution::Unsafe(
            "Texture host lookup and canonical asset identity disagree for backslashes".into(),
        );
    }
    resolve(root, candidate)
}

fn material_texture(kind: ReferenceKind) -> bool {
    matches!(
        kind,
        ReferenceKind::AlbedoTexture
            | ReferenceKind::NormalTexture
            | ReferenceKind::OrmTexture
            | ReferenceKind::EmissiveTexture
            | ReferenceKind::TransparentTexture
    )
}
