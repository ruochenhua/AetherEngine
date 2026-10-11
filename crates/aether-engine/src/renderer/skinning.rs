//! Shared GPU ABI for fixed-size joint palettes.

use crate::asset::gltf_document::MAX_SKELETON_JOINTS;
use bytemuck::cast_slice;
use glam::Mat4;
use wgpu::util::DeviceExt;

pub(crate) const PALETTE_BYTE_SIZE: u64 =
    (MAX_SKELETON_JOINTS * std::mem::size_of::<[[f32; 4]; 4]>()) as u64;

pub(crate) fn create_palette_layout(
    device: &wgpu::Device,
    label: &'static str,
) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: std::num::NonZeroU64::new(PALETTE_BYTE_SIZE),
            },
            count: None,
        }],
    })
}

pub(crate) fn create_identity_palette_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    let matrices = [Mat4::IDENTITY.to_cols_array_2d(); MAX_SKELETON_JOINTS];
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Static Mesh Identity Joint Palette"),
        contents: cast_slice(&matrices),
        usage: wgpu::BufferUsages::STORAGE,
    })
}

pub(crate) fn create_palette_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
    label: &'static str,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    })
}
