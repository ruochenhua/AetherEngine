use crate::asset::mesh::{GpuSkinnedMesh, SkinnedVertex};
use crate::time::animation::{AnimationError, JointPaletteGpu, JointPaletteStorage};
use std::sync::Arc;

/// GPU skin data attached to one opaque render batch.
#[derive(Clone)]
pub struct SkinnedRenderData {
    /// Joint-weighted mesh shared by all geometry consumers.
    pub mesh: Arc<GpuSkinnedMesh>,
    /// Validated 128-matrix palette for this primitive.
    pub palette: Arc<JointPaletteGpu>,
    /// Palette storage buffer kept alive while render passes bind it.
    pub storage: Arc<JointPaletteStorage>,
    /// Binding for the palette storage buffer.
    pub bind_group: Arc<wgpu::BindGroup>,
}

impl SkinnedRenderData {
    /// Upload one primitive and bind the palette ABI used by all consumers.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[SkinnedVertex],
        indices: &[u32],
        palette: JointPaletteGpu,
    ) -> Result<Self, AnimationError> {
        let mesh = Arc::new(GpuSkinnedMesh::new(device, vertices, indices));
        let storage = Arc::new(JointPaletteStorage::new(device));
        storage.upload(queue, &palette)?;
        let bind_group = Arc::new(storage.bind_group(device));
        Ok(Self {
            mesh,
            palette: Arc::new(palette),
            storage,
            bind_group,
        })
    }
}
