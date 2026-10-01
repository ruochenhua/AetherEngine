use super::CpuTexture;
use crate::asset::{Asset, AssetKind};
use std::path::Path;

impl Asset for CpuTexture {
    const KIND: AssetKind = AssetKind::CpuTexture;

    fn load(path: &Path) -> anyhow::Result<Self> {
        Self::from_file(path)
    }
}
