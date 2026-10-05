use super::types::{AnimationError, JointPaletteGpu};
use std::num::NonZeroU64;

/// One fixed-size storage buffer and layout for a primitive's joint palette.
pub struct JointPaletteStorage {
    /// WGSL-compatible 128-matrix storage payload.
    pub buffer: wgpu::Buffer,
    /// Read-only vertex-stage storage layout, suitable for binding in a skinned pipeline.
    pub layout: wgpu::BindGroupLayout,
}

impl JointPaletteStorage {
    /// Allocate a palette buffer and its vertex-stage read-only storage layout.
    pub fn new(device: &wgpu::Device) -> Self {
        let size = std::mem::size_of::<JointPaletteGpu>() as u64;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Animation Joint Palette Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(size),
                },
                count: None,
            }],
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animation Joint Palette"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { buffer, layout }
    }

    /// Create a bind group for the fixed-size palette storage buffer.
    pub fn bind_group(&self, device: &wgpu::Device) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Animation Joint Palette Binding"),
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.buffer.as_entire_binding(),
            }],
        })
    }

    /// Upload one validated palette into this primitive's storage buffer.
    pub fn upload(
        &self,
        queue: &wgpu::Queue,
        palette: &JointPaletteGpu,
    ) -> Result<(), AnimationError> {
        if palette.joint_count == 0 || palette.joint_count > 128 {
            return Err(AnimationError::InvalidPaletteSize(
                palette.joint_count as usize,
            ));
        }
        if palette.matrices[..palette.joint_count as usize]
            .iter()
            .flatten()
            .flatten()
            .any(|value| !value.is_finite())
        {
            return Err(AnimationError::NonFiniteMatrix);
        }
        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(palette));
        Ok(())
    }
}
