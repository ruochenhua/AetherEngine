//! Shared public types for the typed asset store.

use super::super::mesh::CpuMesh;
use super::super::texture::CpuTexture;
use super::super::Asset;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Stable serialized category used in asset identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetKind {
    /// Decoded CPU texture.
    CpuTexture,
    /// Decoded CPU mesh.
    CpuMesh,
    /// Owned glTF document.
    GltfDocument,
    /// Skeleton asset.
    Skeleton,
    /// Animation clip asset.
    AnimationClip,
    /// Material asset.
    Material,
    /// Prefab asset.
    Prefab,
}

/// An existing asset type assigned a stable persistent category for typed storage.
pub trait TypedAsset: Asset + 'static {
    /// Stable category serialized in [`AssetId`].
    const KIND: AssetKind;

    /// Decode while allowing implementations with long-running work to stop early.
    fn load_cancellable(path: &Path, cancellation: &CancellationToken) -> Result<Self, AssetError> {
        if cancellation.is_cancelled() {
            return Err(AssetError::Cancelled);
        }
        let asset = Self::load(path).map_err(|error| AssetError::Decode(error.to_string()))?;
        if cancellation.is_cancelled() {
            return Err(AssetError::Cancelled);
        }
        Ok(asset)
    }
}

/// Read-only cancellation signal shared between the store and a decoder.
#[derive(Debug, Clone)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub(super) fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub(super) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Check whether the request has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

impl TypedAsset for CpuTexture {
    const KIND: AssetKind = AssetKind::CpuTexture;
}

impl TypedAsset for CpuMesh {
    const KIND: AssetKind = AssetKind::CpuMesh;
}

/// Normalized project-relative asset path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CanonicalPath(String);

impl CanonicalPath {
    /// Return the slash-separated normalized path.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Persistent asset identity; independent of Rust type identifiers.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssetId {
    /// Stable serialized asset category.
    pub kind: AssetKind,
    /// Canonical project-relative path.
    pub path: CanonicalPath,
}

impl Serialize for AssetId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&format!("{}:{}", kind_name(self.kind), self.path.as_str()))
    }
}

impl<'de> Deserialize<'de> for AssetId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        let (kind, path) = encoded
            .split_once(':')
            .ok_or_else(|| serde::de::Error::custom("asset id must use kind:path form"))?;
        let kind =
            parse_kind(kind).ok_or_else(|| serde::de::Error::custom("unknown asset kind"))?;
        let path =
            canonicalize(Path::new("."), Path::new(path)).map_err(serde::de::Error::custom)?;
        Ok(Self { kind, path })
    }
}

fn kind_name(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::CpuTexture => "CpuTexture",
        AssetKind::CpuMesh => "CpuMesh",
        AssetKind::GltfDocument => "GltfDocument",
        AssetKind::Skeleton => "Skeleton",
        AssetKind::AnimationClip => "AnimationClip",
        AssetKind::Material => "Material",
        AssetKind::Prefab => "Prefab",
    }
}

fn parse_kind(name: &str) -> Option<AssetKind> {
    Some(match name {
        "CpuTexture" => AssetKind::CpuTexture,
        "CpuMesh" => AssetKind::CpuMesh,
        "GltfDocument" => AssetKind::GltfDocument,
        "Skeleton" => AssetKind::Skeleton,
        "AnimationClip" => AssetKind::AnimationClip,
        "Material" => AssetKind::Material,
        "Prefab" => AssetKind::Prefab,
        _ => return None,
    })
}

impl std::fmt::Display for AssetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}:{}", self.kind, self.path.as_str())
    }
}

/// Normalize a project-relative path and reject paths escaping the root.
pub fn canonicalize(_project_root: &Path, input: &Path) -> Result<CanonicalPath, AssetError> {
    if input.is_absolute() {
        return Err(AssetError::PathEscape);
    }
    let mut parts = Vec::new();
    for component in input.components() {
        match component {
            Component::Normal(part) => {
                parts.push(part.to_str().ok_or(AssetError::UnsupportedKind)?.to_owned())
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.pop().is_none() {
                    return Err(AssetError::PathEscape);
                }
            }
            Component::RootDir | Component::Prefix(_) => return Err(AssetError::PathEscape),
        }
    }
    if parts.is_empty() {
        return Err(AssetError::PathEscape);
    }
    Ok(CanonicalPath(parts.join("/")))
}

/// Asset lifecycle errors.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum AssetError {
    /// Path is absolute or escapes the configured project root.
    #[error("asset path escapes project root")]
    PathEscape,
    /// Asset kind or path cannot be represented by the stable identity schema.
    #[error("unsupported asset kind or path")]
    UnsupportedKind,
    /// Asset decoding failed.
    #[error("asset decode failed: {0}")]
    Decode(String),
    /// Handle refers to a released or superseded slot.
    #[error("stale asset handle")]
    StaleHandle,
    /// Reload ticket is stale or was already superseded.
    #[error("stale asset reload ticket")]
    StaleReloadTicket,
    /// Worker no longer accepts requests.
    #[error("asset worker has stopped")]
    WorkerStopped,
    /// Request was cancelled before its result could be applied.
    #[error("asset request was cancelled")]
    Cancelled,
    /// Payload category does not match its requested category.
    #[error("asset payload kind mismatch")]
    PayloadKindMismatch,
    /// Payload concrete type does not match the requested type.
    #[error("asset payload type mismatch")]
    PayloadTypeMismatch,
    /// Frame boundary is not newer than the last applied boundary.
    #[error("invalid frame boundary")]
    InvalidFrameBoundary,
    /// A frame commit was attempted from a thread other than the store owner.
    #[error("asset commit must run on the store owner thread")]
    WrongCommitThread,
    /// Renderer-side asset commit failed before the generation was published.
    #[error("asset commit failed: {0}")]
    CommitFailed(String),
    /// Worker did not stop before configured timeout.
    #[error("asset worker shutdown timed out")]
    ShutdownTimeout,
}

/// Current observable lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadStateView {
    /// A decode is in flight.
    Loading {
        /// Generation currently being loaded.
        generation: u32,
    },
    /// A decoded payload is available.
    Ready {
        /// Current payload generation.
        generation: u32,
    },
    /// Latest decode failed; an older payload may still be available.
    Failed {
        /// Generation associated with the failed request.
        generation: u32,
        /// Failure reason.
        error: AssetError,
        /// Whether a previously successful payload remains available.
        has_last_good: bool,
    },
}

/// Main-thread frame boundary for applying decoded results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameBoundary {
    /// Monotonically increasing rendered frame id.
    pub frame_id: u64,
}

/// Asset worker configuration.
#[derive(Debug, Clone)]
pub struct AssetStoreConfig {
    /// Project root used to resolve canonical relative paths.
    pub project_root: PathBuf,
    /// Maximum time spent waiting for worker shutdown.
    pub join_timeout: Duration,
}

impl AssetStoreConfig {
    /// Build a configuration with a short bounded worker shutdown wait.
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            join_timeout: Duration::from_millis(100),
        }
    }
}

/// Opaque cancellation capability returned by a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LoadTicket(pub(super) u128);

/// Versioned result of a load or reload.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReloadTicket {
    /// Stable asset key.
    pub asset: AssetId,
    /// Generation before successful application.
    pub from_generation: u32,
    /// Generation produced by successful application.
    pub to_generation: u32,
    /// Monotonic operation sequence.
    pub sequence: u64,
    pub(super) slot: u64,
    pub(super) capability: u128,
}

/// Type-erased but ownership-safe payload.
pub trait AssetPayload: Send + Sync {
    /// Stable asset kind carried by this payload.
    fn kind(&self) -> AssetKind;
    /// Borrow the concrete value for diagnostics and validation.
    fn as_any(&self) -> &dyn Any;
    /// Consume the shared payload while retaining owned `Arc` semantics.
    fn into_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
}

impl<T: TypedAsset> AssetPayload for T {
    fn kind(&self) -> AssetKind {
        T::KIND
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn into_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}

/// Safely downcast an owned payload without constructing an `Arc` from a borrow.
pub fn downcast_owned<T: TypedAsset>(payload: Arc<dyn AssetPayload>) -> Result<Arc<T>, AssetError> {
    if payload.kind() != T::KIND {
        return Err(AssetError::PayloadKindMismatch);
    }
    Arc::downcast::<T>(payload.into_any()).map_err(|_| AssetError::PayloadTypeMismatch)
}

/// Worker result waiting for main-thread application.
pub struct AssetResult {
    pub(super) slot: u64,
    pub(super) generation: u32,
    pub(super) sequence: u64,
    pub(super) ticket: u128,
    pub(super) kind: AssetKind,
    pub(super) result: Result<Arc<dyn AssetPayload>, AssetError>,
}

impl AssetResult {
    pub(crate) fn slot_id(&self) -> u64 {
        self.slot
    }
}
