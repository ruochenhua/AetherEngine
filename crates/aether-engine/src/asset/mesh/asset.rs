use super::CpuMesh;
use crate::asset::{Asset, AssetKind};
use std::path::Path;

impl Asset for CpuMesh {
    const KIND: AssetKind = AssetKind::CpuMesh;

    fn load(path: &Path) -> anyhow::Result<Self> {
        crate::asset::loaders::load_mesh(path)
    }
}
