use super::PrefabDocument;
use crate::asset::{Asset, AssetKind};
use std::path::Path;

impl Asset for PrefabAsset {
    const KIND: AssetKind = AssetKind::Prefab;

    fn load(path: &Path) -> anyhow::Result<Self> {
        let source = std::fs::read_to_string(path)?;
        Ok(Self {
            document: PrefabDocument::from_ron(&source)?,
        })
    }
}

/// Typed AssetStore payload for a Prefab document.
#[derive(Clone, Debug)]
pub struct PrefabAsset {
    /// Validated, migrated Prefab document.
    pub document: PrefabDocument,
}
