//! Stable identifiers for typed assets.

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::path::{Component, Path};
use std::str::FromStr;
use thiserror::Error;

/// Stable asset categories persisted in scene and project metadata.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AssetKind {
    /// Decoded CPU texture data.
    CpuTexture,
    /// Decoded CPU mesh data.
    CpuMesh,
    /// Parsed glTF document data.
    GltfDocument,
    /// Derived skeleton data.
    Skeleton,
    /// Derived animation clip data.
    AnimationClip,
    /// Material configuration data.
    Material,
    /// Prefab document data.
    Prefab,
}

impl AssetKind {
    /// Return the stable lowercase spelling used in serialized identifiers.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CpuTexture => "cpu_texture",
            Self::CpuMesh => "cpu_mesh",
            Self::GltfDocument => "gltf_document",
            Self::Skeleton => "skeleton",
            Self::AnimationClip => "animation_clip",
            Self::Material => "material",
            Self::Prefab => "prefab",
        }
    }
}

impl fmt::Display for AssetKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for AssetKind {
    type Err = AssetIdParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "cpu_texture" => Ok(Self::CpuTexture),
            "cpu_mesh" => Ok(Self::CpuMesh),
            "gltf_document" => Ok(Self::GltfDocument),
            "skeleton" => Ok(Self::Skeleton),
            "animation_clip" => Ok(Self::AnimationClip),
            "material" => Ok(Self::Material),
            "prefab" => Ok(Self::Prefab),
            _ => Err(AssetIdParseError::UnknownKind(value.to_owned())),
        }
    }
}

impl Serialize for AssetKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for AssetKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(de::Error::custom)
    }
}

/// A normalized project-relative path with slash separators.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CanonicalPath(String);

impl CanonicalPath {
    /// Normalize a relative asset path and reject paths escaping the project root.
    pub fn new(project_root: &Path, input: &Path) -> Result<Self, AssetIdError> {
        let _ = project_root;
        let raw = input.to_str().ok_or(AssetIdError::InvalidPath)?;
        let slash_path = raw.replace('\\', "/");
        if slash_path.is_empty()
            || slash_path.starts_with('/')
            || has_windows_drive_prefix(&slash_path)
        {
            return Err(AssetIdError::InvalidPath);
        }

        let mut components = Vec::new();
        for component in slash_path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    if components.pop().is_none() {
                        return Err(AssetIdError::PathEscape);
                    }
                }
                value => components.push(value),
            }
        }

        if components.is_empty() {
            return Err(AssetIdError::InvalidPath);
        }
        Ok(Self(components.join("/")))
    }

    pub(crate) fn for_legacy_external_path(input: &Path) -> Result<Self, AssetIdError> {
        if !input.is_absolute() {
            return Err(AssetIdError::InvalidPath);
        }
        let mut components = vec!["external".to_owned()];
        for component in input.components() {
            match component {
                Component::Prefix(prefix) => {
                    let value = prefix
                        .as_os_str()
                        .to_string_lossy()
                        .replace('\\', "/")
                        .replace(':', "")
                        .to_ascii_lowercase();
                    if !value.is_empty() {
                        components.push(value);
                    }
                }
                Component::RootDir | Component::CurDir => {}
                Component::ParentDir => {
                    if components.len() <= 1 {
                        return Err(AssetIdError::PathEscape);
                    }
                    components.pop();
                }
                Component::Normal(value) => {
                    let value = value.to_str().ok_or(AssetIdError::InvalidPath)?;
                    components.push(value.to_owned());
                }
            }
        }
        if components.len() <= 1 {
            return Err(AssetIdError::InvalidPath);
        }
        Ok(Self(components.join("/")))
    }

    /// Return the normalized project-relative path string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CanonicalPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for CanonicalPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CanonicalPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        CanonicalPath::new(Path::new("."), Path::new(&value)).map_err(de::Error::custom)
    }
}

/// Persistent typed asset identity, serialized as `kind:canonical/path`.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct AssetId {
    kind: AssetKind,
    path: CanonicalPath,
}

impl AssetId {
    /// Construct an identifier from an asset kind and normalized path.
    pub fn new(kind: AssetKind, path: CanonicalPath) -> Self {
        Self { kind, path }
    }

    /// Construct an identifier by normalizing a path relative to a project root.
    pub fn from_path(
        kind: AssetKind,
        project_root: &Path,
        path: &Path,
    ) -> Result<Self, AssetIdError> {
        Ok(Self::new(kind, CanonicalPath::new(project_root, path)?))
    }

    pub(crate) fn from_legacy_external_path(
        kind: AssetKind,
        path: &Path,
    ) -> Result<Self, AssetIdError> {
        Ok(Self::new(
            kind,
            CanonicalPath::for_legacy_external_path(path)?,
        ))
    }

    /// Return the stable asset category.
    pub fn kind(&self) -> AssetKind {
        self.kind
    }

    /// Return the normalized project-relative path.
    pub fn path(&self) -> &CanonicalPath {
        &self.path
    }
}

impl fmt::Display for AssetId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.kind, self.path)
    }
}

impl FromStr for AssetId {
    type Err = AssetIdParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (kind, path) = value
            .split_once(':')
            .ok_or(AssetIdParseError::MissingSeparator)?;
        let kind = kind.parse()?;
        let path = CanonicalPath::new(Path::new("."), Path::new(path))?;
        Ok(Self { kind, path })
    }
}

impl Serialize for AssetId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for AssetId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(de::Error::custom)
    }
}

/// Asset identity construction or parsing failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AssetIdError {
    /// A path was absolute, empty, or could not be represented as UTF-8.
    #[error("asset path must be a non-empty relative UTF-8 path")]
    InvalidPath,
    /// Normalizing the path would escape the project root.
    #[error("asset path escapes the project root")]
    PathEscape,
}

/// Serialized asset identifier parsing failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AssetIdParseError {
    /// The serialized identifier omitted its kind/path separator.
    #[error("asset identifier must have the form kind:path")]
    MissingSeparator,
    /// The identifier used an unrecognized persistent asset kind.
    #[error("unknown asset kind: {0}")]
    UnknownKind(String),
    /// The path portion was invalid or escaped its root.
    #[error(transparent)]
    InvalidPath(#[from] AssetIdError),
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_path_normalizes_slashes_and_inner_parent_segments() {
        let path = CanonicalPath::new(
            Path::new("project"),
            Path::new("models\\ships/../ship.gltf"),
        )
        .unwrap();
        assert_eq!(path.as_str(), "models/ship.gltf");
    }

    #[test]
    fn canonical_path_rejects_absolute_and_root_escape_paths() {
        assert_eq!(
            CanonicalPath::new(Path::new("project"), Path::new("../outside.png")),
            Err(AssetIdError::PathEscape)
        );
        assert_eq!(
            CanonicalPath::new(Path::new("project"), Path::new("C:\\assets\\ship.png")),
            Err(AssetIdError::InvalidPath)
        );
        assert_eq!(
            CanonicalPath::new(Path::new("project"), Path::new("/assets/ship.png")),
            Err(AssetIdError::InvalidPath)
        );
    }

    #[test]
    fn asset_id_serialization_is_stable_and_kind_sensitive() {
        let texture = AssetId::from_path(
            AssetKind::CpuTexture,
            Path::new("project"),
            Path::new("textures/shared.png"),
        )
        .unwrap();
        let mesh = AssetId::from_path(
            AssetKind::CpuMesh,
            Path::new("project"),
            Path::new("textures/shared.png"),
        )
        .unwrap();
        assert_eq!(texture.to_string(), "cpu_texture:textures/shared.png");
        assert_ne!(texture, mesh);
        assert_eq!(texture, texture.to_string().parse().unwrap());
        assert_eq!(
            ron::to_string(&texture).unwrap(),
            "\"cpu_texture:textures/shared.png\""
        );
        assert_eq!(
            ron::from_str::<AssetId>(&ron::to_string(&texture).unwrap()).unwrap(),
            texture
        );
    }
}
