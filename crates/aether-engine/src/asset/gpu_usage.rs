//! Read-only summaries of GPU cache ownership.

use super::AssetId;

/// Whether a cache inventory could be read in full.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuCacheInventoryState {
    /// The reported cache entries were read successfully.
    Available,
    /// At least one cache lock was poisoned, so entries may be missing.
    Unavailable,
}

/// The runtime cache retaining a GPU resource generation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum GpuResourceOwner {
    /// A GPU texture retained by the texture cache.
    Texture,
    /// A GPU material retained by the material cache.
    Material,
}

/// One path-addressable generation retained by a GPU cache.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuResourceUsage {
    /// Cache that owns this generation.
    pub owner: GpuResourceOwner,
    /// Persistent asset identity.
    pub asset: AssetId,
    /// Uploaded source generation.
    pub generation: u32,
    /// Whether this generation is the cache's current generation.
    pub current: bool,
    /// Texture color interpretation; absent for materials.
    pub srgb: Option<bool>,
    /// References beyond those retained by the cache maps.
    pub external_arc_owners: usize,
}

/// Owned, deterministic inventory from one GPU cache.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuResourceUsageSnapshot {
    /// Whether the inventory is complete.
    pub state: GpuCacheInventoryState,
    /// Path-addressable cache generations, sorted by owner, path, generation, and color mode.
    pub resources: Vec<GpuResourceUsage>,
    /// Number of legacy texture entries whose persistent asset identity is unknown.
    /// `None` means the legacy map could not be read.
    pub unidentified_legacy_texture_entries: Option<usize>,
}

pub(crate) fn sort_resources(resources: &mut [GpuResourceUsage]) {
    resources.sort_by(|left, right| {
        (
            left.owner,
            left.asset.kind().as_str(),
            left.asset.path().as_str(),
            left.generation,
            left.srgb,
        )
            .cmp(&(
                right.owner,
                right.asset.kind().as_str(),
                right.asset.path().as_str(),
                right.generation,
                right.srgb,
            ))
    });
}
