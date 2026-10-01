use super::AssetKind;
use std::path::Path;

/// Trait for loadable typed assets.
pub trait Asset: Sized + Send + Sync + 'static {
    /// Stable category used by persisted asset identifiers.
    const KIND: AssetKind;

    /// Load the asset from a file path.
    fn load(path: &Path) -> anyhow::Result<Self>;
}
