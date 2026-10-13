//! Bounded immutable CPU texture identities for host material preparation.
use super::AssetManager;
use crate::asset::{texture::CpuTexture, Handle};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf};

/// Global snapshot budgets for one AssetManager, excluding ordinary source assets.
#[derive(Clone, Copy, Debug)]
pub struct TextureSnapshotLimits {
    /// Maximum retained decoded pixel bytes.
    pub decoded_bytes: usize,
    /// Maximum number of unique retained snapshots.
    pub count: usize,
}
impl Default for TextureSnapshotLimits {
    fn default() -> Self {
        Self {
            decoded_bytes: 256 * 1024 * 1024,
            count: 256,
        }
    }
}

#[derive(Default)]
pub(super) struct SnapshotCache {
    handles: BTreeMap<String, Handle<CpuTexture>>,
    bytes: usize,
    pub(super) limits: TextureSnapshotLimits,
}

fn identity(texture: &CpuTexture) -> String {
    let mut hash = Sha256::new();
    hash.update(b"aether-editor-texture-snapshot-v1\0");
    hash.update(texture.width.to_le_bytes());
    hash.update(texture.height.to_le_bytes());
    hash.update(texture.channels.to_le_bytes());
    hash.update(format!("{:?}\0", texture.format).as_bytes());
    hash.update(&texture.data);
    format!("__editor_texture_snapshots__/{:x}", hash.finalize())
}

impl AssetManager {
    /// Current number of unique immutable snapshots retained by this manager.
    pub fn texture_snapshot_count(&self) -> usize {
        self.texture_snapshots.handles.len()
    }

    /// Current decoded pixel bytes retained by immutable snapshots.
    pub fn texture_snapshot_bytes(&self) -> usize {
        self.texture_snapshots.bytes
    }

    /// Register one host-validated texture under its immutable content identity.
    pub fn register_texture_snapshot(
        &mut self,
        texture: CpuTexture,
    ) -> anyhow::Result<Handle<CpuTexture>> {
        self.register_texture_snapshots(vec![texture])?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("snapshot registration returned no handle"))
    }

    /// Atomically budget and install host-validated decoded textures.
    ///
    /// Equal content deduplicates within the batch and against existing snapshots.
    /// Rejection leaves all counts, payloads, and existing handles unchanged.
    /// Outputs follow input order. Runtime identities never replace scene paths.
    pub fn register_texture_snapshots(
        &mut self,
        textures: Vec<CpuTexture>,
    ) -> anyhow::Result<Vec<Handle<CpuTexture>>> {
        let mut requested = Vec::with_capacity(textures.len());
        let mut unique = BTreeMap::new();
        for texture in textures {
            let key = identity(&texture);
            if !self.texture_snapshots.handles.contains_key(&key) {
                unique.entry(key.clone()).or_insert(texture);
            }
            requested.push(key);
        }
        let bytes = unique
            .values()
            .try_fold(self.texture_snapshots.bytes, |total, texture| {
                total.checked_add(texture.data.len())
            })
            .ok_or_else(|| anyhow::anyhow!("texture snapshot decoded byte budget overflow"))?;
        let count = self
            .texture_snapshot_count()
            .checked_add(unique.len())
            .ok_or_else(|| anyhow::anyhow!("texture snapshot count budget overflow"))?;
        anyhow::ensure!(
            bytes <= self.texture_snapshots.limits.decoded_bytes,
            "texture snapshot decoded byte budget exceeded ({} bytes)",
            self.texture_snapshots.limits.decoded_bytes
        );
        anyhow::ensure!(
            count <= self.texture_snapshots.limits.count,
            "texture snapshot count budget exceeded ({} snapshots)",
            self.texture_snapshots.limits.count
        );
        let keys: Vec<_> = unique.keys().cloned().collect();
        let entries = unique
            .into_iter()
            .map(|(key, texture)| (PathBuf::from(key), texture))
            .collect();
        let handles = self.store.insert_ready_batch(entries)?;
        self.texture_snapshots
            .handles
            .extend(keys.into_iter().zip(handles));
        self.texture_snapshots.bytes = bytes;
        requested
            .into_iter()
            .map(|key| {
                self.texture_snapshots
                    .handles
                    .get(&key)
                    .copied()
                    .ok_or_else(|| {
                        anyhow::anyhow!("snapshot registration returned no matching handle")
                    })
            })
            .collect()
    }
}
