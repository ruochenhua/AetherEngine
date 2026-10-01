//! Public contracts and internal records shared by the typed asset store.

use super::{Asset, AssetId, AssetIdError, AssetKind};
use std::any::{Any, TypeId};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use thiserror::Error;

/// Errors returned by typed asset lifecycle operations.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AssetError {
    /// The asset path is invalid or escapes the configured project root.
    #[error(transparent)]
    InvalidPath(#[from] AssetIdError),
    /// The handle no longer refers to the current asset generation.
    #[error("asset handle is stale")]
    StaleHandle,
    /// The load or reload ticket no longer identifies the active request.
    #[error("asset reload ticket is stale")]
    StaleReloadTicket,
    /// The asset has not completed its current load yet.
    #[error("asset is not ready")]
    NotReady,
    /// The file decoder failed while loading the asset.
    #[error("asset decode failed: {0}")]
    Decode(String),
    /// A dependent asset failed to load.
    #[error("asset dependency {asset} failed: {cause}")]
    Dependency {
        /// Asset that failed to resolve.
        asset: AssetId,
        /// Underlying load failure.
        cause: Box<AssetError>,
    },
    /// The worker has stopped or the store is shutting down.
    #[error("asset worker is stopped")]
    WorkerStopped,
    /// The load was cancelled before its result could be applied.
    #[error("asset load was cancelled")]
    Cancelled,
    /// The erased payload kind does not match the persistent asset identity.
    #[error("asset payload kind does not match its identifier")]
    PayloadKindMismatch,
    /// The erased payload has the expected kind but the wrong Rust type.
    #[error("asset payload type does not match the requested type")]
    PayloadTypeMismatch,
    /// A frame boundary moved backwards.
    #[error("asset results must be applied at a non-decreasing frame boundary")]
    InvalidFrameBoundary,
    /// A slot or sequence counter cannot be represented.
    #[error("asset store identifier space is exhausted")]
    IdentifierExhausted,
    /// No GPU cache accepts this asset kind.
    #[error("no GPU cache supports asset kind {0}")]
    UnsupportedKind(AssetKind),
    /// The GPU cache rejected an upload or commit.
    #[error("GPU asset commit failed: {0}")]
    GpuCommit(String),
    /// The worker did not stop before the configured shutdown deadline.
    #[error("asset worker did not stop before the shutdown timeout")]
    ShutdownTimeout,
}

/// State visible to callers holding a typed asset handle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoadStateView {
    /// The requested generation is being decoded on the worker.
    Loading {
        /// Generation currently being loaded.
        generation: u32,
    },
    /// The requested generation is available to CPU consumers.
    Ready {
        /// Generation currently available.
        generation: u32,
    },
    /// The latest request failed; an earlier generation may still be usable.
    Failed {
        /// Generation whose load failed.
        generation: u32,
        /// Failure details.
        error: AssetError,
        /// Whether `get` can still return a last-known-good generation.
        has_last_good: bool,
    },
}

/// Opaque, caller-owned capability for cancelling a queued asset request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LoadTicket {
    pub(super) id: u128,
}

/// Versioned commit capability for one successfully applied asset result.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ReloadTicket {
    /// Persistent asset identity.
    pub asset: AssetId,
    /// Generation currently active before the request.
    pub from_generation: u32,
    /// Generation produced by the request.
    pub to_generation: u32,
    /// Monotonic result sequence allocated by this store.
    pub sequence: u64,
    pub(super) load_ticket: LoadTicket,
}

/// Cancellation flag passed to a worker without exposing its constructor.
#[derive(Clone, Debug)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub(super) fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    /// Return whether the caller has cancelled this request.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub(super) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
}

/// Immutable request metadata carried from the worker to the app thread.
#[derive(Clone, Debug)]
pub struct LoadRequest {
    /// Persistent identity of the requested asset.
    pub asset: AssetId,
    /// Handle slot allocated to the identity.
    pub slot: u32,
    /// Generation requested by this load or reload.
    pub generation: u32,
    /// Monotonic request order within the store.
    pub sequence: u64,
    /// Cancellation state shared with the store.
    pub cancel: CancellationToken,
    /// Opaque cancellation capability returned to the caller.
    pub(super) ticket: LoadTicket,
    /// Generation active before this request began.
    pub(super) from_generation: u32,
    /// Runtime Rust type expected for this request; never persisted as an asset key.
    pub(super) expected_type: TypeId,
}

impl LoadRequest {
    /// Return the opaque ticket associated with this worker request.
    pub fn ticket(&self) -> LoadTicket {
        self.ticket
    }
}

/// A decoded CPU payload with a stable kind and owned downcast path.
pub trait AssetPayload: Send + Sync {
    /// Return the persistent kind of this payload.
    fn kind(&self) -> AssetKind;
    /// Borrow the concrete value for a non-owning type check.
    fn as_any(&self) -> &dyn Any;
    /// Consume an owned payload handle for `Arc::downcast`.
    fn into_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
}

impl<T: Asset> AssetPayload for T {
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

/// Result emitted by an asset worker and consumed on the app thread.
pub enum AssetResult {
    /// Decoding completed successfully.
    Ready {
        /// Original request metadata.
        request: LoadRequest,
        /// Owned decoded CPU payload.
        payload: Arc<dyn AssetPayload>,
    },
    /// Decoding failed.
    Failed {
        /// Original request metadata.
        request: LoadRequest,
        /// Decoder or cancellation failure.
        error: AssetError,
    },
}

/// Summary returned after a worker result is accepted at a frame boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplyOutcome {
    /// Persistent identity that changed state.
    pub asset: AssetId,
    /// Ticket describing the applied version.
    pub ticket: ReloadTicket,
    /// State visible after applying the result.
    pub state: LoadStateView,
    /// Payload kind when a last-known-good CPU value exists.
    pub payload_kind: Option<AssetKind>,
}

impl AssetResult {
    /// Borrow request metadata without consuming the result.
    pub fn request(&self) -> &LoadRequest {
        match self {
            Self::Ready { request, .. } | Self::Failed { request, .. } => request,
        }
    }
}

/// Monotonic app/render-frame boundary used to apply CPU and GPU changes.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FrameBoundary {
    /// Frame identifier supplied by the app loop.
    pub frame_id: u64,
}

impl FrameBoundary {
    /// Create a boundary for the given app frame.
    pub fn new(frame_id: u64) -> Self {
        Self { frame_id }
    }
}

/// Versioned GPU cache key; old generations remain addressable while referenced.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct GpuAssetKey {
    /// Persistent typed identity.
    pub asset: AssetId,
    /// CPU generation uploaded for this key.
    pub generation: u32,
}

/// Adapter implemented by GPU caches that can commit typed payloads.
pub trait GpuCommitCache {
    /// Upload or otherwise commit one asset at an app-owned frame boundary.
    fn commit_asset(
        &mut self,
        key: GpuAssetKey,
        payload: Arc<dyn AssetPayload>,
        boundary: FrameBoundary,
    ) -> Result<(), AssetError>;

    /// Drop cache-owned references to generations no longer in use.
    fn collect_retired(&mut self);
}

/// GPU commit resources and the frame boundary currently being applied.
pub struct GpuCommitContext<'a> {
    /// Cache owning GPU allocations.
    pub cache: &'a mut dyn GpuCommitCache,
    /// App/render frame boundary for this commit.
    pub boundary: FrameBoundary,
}

/// Configuration for a typed asset store.
#[derive(Clone, Debug)]
pub struct AssetStoreConfig {
    /// Root used to resolve canonical project-relative paths.
    pub project_root: PathBuf,
    /// Maximum time `shutdown` waits for the worker before returning a timeout.
    pub join_timeout_ms: u32,
}

impl AssetStoreConfig {
    /// Create a config rooted at `project_root` with a one second shutdown wait.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
            join_timeout_ms: 1_000,
        }
    }
}

impl Default for AssetStoreConfig {
    fn default() -> Self {
        let current = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let project_root = current
            .ancestors()
            .find(|candidate| {
                std::fs::read_to_string(candidate.join("Cargo.toml"))
                    .is_ok_and(|manifest| manifest.lines().any(|line| line.trim() == "[workspace]"))
            })
            .map(PathBuf::from)
            .unwrap_or(current);
        Self::new(project_root)
    }
}

pub(super) struct TicketRecord {
    pub slot: u32,
    pub generation: u32,
    pub sequence: u64,
    pub token: CancellationToken,
    pub cancelled: bool,
}

pub(super) struct PendingLoad {
    pub request: LoadRequest,
    pub reload: ReloadTicket,
}

pub(super) struct AssetEntry {
    pub id: AssetId,
    pub current_generation: u32,
    pub requested_generation: u32,
    pub state: LoadStateView,
    pub current: Option<Arc<dyn AssetPayload>>,
    pub retired: Vec<(u32, Arc<dyn AssetPayload>)>,
    pub pending: Option<PendingLoad>,
    pub last_ticket: Option<LoadTicket>,
    pub applied: Option<ReloadTicket>,
    pub committed_sequences: Vec<u64>,
}

impl AssetEntry {
    pub fn new(id: AssetId) -> Self {
        Self {
            id,
            current_generation: 0,
            requested_generation: 0,
            state: LoadStateView::Failed {
                generation: 0,
                error: AssetError::NotReady,
                has_last_good: false,
            },
            current: None,
            retired: Vec::new(),
            pending: None,
            last_ticket: None,
            applied: None,
            committed_sequences: Vec::new(),
        }
    }
}
