//! Immutable CPU texture identities for transactional host material preparation.
use super::AssetManager;
use crate::asset::{texture::CpuTexture, Handle};
use sha2::{Digest, Sha256};
use std::path::Path;

impl AssetManager {
    /// Register decoded pixels under an immutable, content-addressed runtime identity.
    ///
    /// The host validates files before calling this method. Equal pixels/metadata
    /// reuse one handle. Different versions retain distinct ready identities, so
    /// other consumers and transaction rollback never receive stale handles.
    /// These internal identities must not replace source paths in scene files.
    pub fn register_texture_snapshot(
        &mut self,
        texture: CpuTexture,
    ) -> anyhow::Result<Handle<CpuTexture>> {
        let mut hash = Sha256::new();
        hash.update(b"aether-editor-texture-snapshot-v1\0");
        hash.update(texture.width.to_le_bytes());
        hash.update(texture.height.to_le_bytes());
        hash.update(texture.channels.to_le_bytes());
        hash.update(format!("{:?}\0", texture.format).as_bytes());
        hash.update(&texture.data);
        let path = format!("__editor_texture_snapshots__/{:x}", hash.finalize());
        self.store
            .insert_ready(Path::new(&path), texture)
            .map_err(anyhow::Error::from)
    }
}
