//! General transparent geometry pass.

use crate::asset::mesh::GpuMesh;
use crate::particles::render::{pack_items, ParticleGpuItem};
use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{InitContext, Pass, PassSignature, ResHandle};
use crate::renderer::resource::{GDepth, SceneColor, TransparentColor};
use crate::renderer::resource_table::ResourceTable;
use crate::renderer::transparent::{TransparentBlendMode, TransparentMaterial};
use glam::Mat4;
use std::collections::HashMap;
use std::sync::Arc;

mod pipeline;
mod shaders;

/// Dynamic per-draw data for the transparent forward overlay.
#[repr(C, align(256))]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct TransparentObjectUniform {
    /// Model matrix.
    pub model: [[f32; 4]; 4],
    /// Straight-alpha linear color; the shader premultiplies RGB.
    pub color: [f32; 4],
    /// Alpha discard threshold.
    pub alpha_cutoff: f32,
    /// Whether alpha cutoff is enabled.
    pub cutoff_enabled: u32,
    /// Explicit padding to the dynamic-uniform alignment.
    pub _padding: [u32; 42],
}

/// One transparent mesh instance after camera-depth extraction.
struct TransparentDrawItem {
    entity_id: u32,
    depth: f32,
    mesh: Arc<GpuMesh>,
    model: Mat4,
    material: TransparentMaterial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransparentDrawSource {
    Mesh(usize),
    Particle(usize),
}

fn sort_mixed_draw_order(
    mesh_depths: &[f32],
    particle_depths: &[f32],
) -> Vec<TransparentDrawSource> {
    let mut keyed: Vec<_> = mesh_depths
        .iter()
        .enumerate()
        .map(|(index, depth)| (*depth, 0u8, index, TransparentDrawSource::Mesh(index)))
        .chain(
            particle_depths
                .iter()
                .enumerate()
                .map(|(index, depth)| (*depth, 1u8, index, TransparentDrawSource::Particle(index))),
        )
        .collect();
    keyed.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    keyed.into_iter().map(|(_, _, _, source)| source).collect()
}

/// General transparent overlay pass.
pub struct TransparentPass {
    device: wgpu::Device,
    alpha_pipeline: wgpu::RenderPipeline,
    additive_pipeline: wgpu::RenderPipeline,
    view_proj_buffer: wgpu::Buffer,
    view_proj_bind_group: wgpu::BindGroup,
    object_buffer: wgpu::Buffer,
    object_buffer_capacity: usize,
    object_bind_group: wgpu::BindGroup,
    object_bind_group_layout: wgpu::BindGroupLayout,
    particle_alpha_pipeline: wgpu::RenderPipeline,
    particle_additive_pipeline: wgpu::RenderPipeline,
    particle_buffer: wgpu::Buffer,
    particle_buffer_capacity: usize,
    particle_bind_group: wgpu::BindGroup,
    particle_bind_group_layout: wgpu::BindGroupLayout,
    particle_texture_bind_group_layout: wgpu::BindGroupLayout,
    particle_texture_bind_groups: HashMap<u64, wgpu::BindGroup>,
    particle_texture_ids: Vec<u64>,
    particle_depths: Vec<f32>,
    particle_additive: Vec<bool>,
    transparent_color_handle: Option<ResHandle<TransparentColor>>,
    depth_handle: Option<ResHandle<GDepth>>,
    draws: Vec<TransparentDrawItem>,
    draw_order: Vec<TransparentDrawSource>,
}

impl TransparentPass {
    fn sort_draws(draws: &mut [TransparentDrawItem]) {
        draws.sort_by(|left, right| {
            right
                .depth
                .total_cmp(&left.depth)
                .then_with(|| left.entity_id.cmp(&right.entity_id))
        });
    }

    #[cfg(test)]
    fn signature_for_test() -> PassSignature {
        PassSignature::new("Transparent")
            .read::<SceneColor>()
            .read::<GDepth>()
            .write::<TransparentColor>(wgpu::TextureFormat::Rgba16Float)
    }
}

impl Pass for TransparentPass {
    fn name(&self) -> &str {
        "Transparent"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("Transparent")
            .read::<SceneColor>()
            .read::<GDepth>()
            .write::<TransparentColor>(wgpu::TextureFormat::Rgba16Float)
    }

    fn init(ctx: &InitContext) -> Self {
        Self::new(ctx.device)
    }

    fn resolve(&mut self, _device: &wgpu::Device, resources: &ResourceTable) {
        self.transparent_color_handle = Some(resources.handle::<TransparentColor>());
        self.depth_handle = Some(resources.handle::<GDepth>());
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        let view = frame.camera.view_matrix();
        let proj = frame.camera.projection_matrix(frame.aspect);
        let vp = crate::renderer::renderable::ViewProjUniform {
            view: view.to_cols_array_2d(),
            proj: proj.to_cols_array_2d(),
        };
        frame
            .queue
            .write_buffer(&self.view_proj_buffer, 0, bytemuck::bytes_of(&vp));

        let particle_items = frame
            .optional
            .particles
            .as_deref()
            .map(|snapshot| pack_items(snapshot, view))
            .unwrap_or_default();
        self.particle_additive = particle_items
            .iter()
            .map(|item| item.gpu.blend != 0)
            .collect();
        self.particle_texture_ids.clear();
        self.particle_depths = particle_items
            .iter()
            .map(|item| item.gpu.rotation_depth[1])
            .collect();
        for item in &particle_items {
            let texture = item
                .texture
                .as_ref()
                .filter(|handle| frame.asset_manager.is_loaded((*handle).clone()))
                .cloned();
            let texture_id = texture.as_ref().map_or(0, |handle| handle.id());
            self.particle_texture_ids.push(texture_id);
            if !self.particle_texture_bind_groups.contains_key(&texture_id) {
                let texture = frame
                    .texture_cache
                    .get_or_upload_optional(texture, frame.asset_manager);
                let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Particle Billboard Texture Bind Group"),
                    layout: &self.particle_texture_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&texture.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&texture.sampler),
                        },
                    ],
                });
                self.particle_texture_bind_groups
                    .insert(texture_id, bind_group);
            }
        }
        if particle_items.len() > self.particle_buffer_capacity {
            self.particle_buffer_capacity = particle_items.len().next_power_of_two();
            self.particle_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Particle Billboard Instance Buffer"),
                size: (std::mem::size_of::<ParticleGpuItem>() * self.particle_buffer_capacity)
                    as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.particle_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Particle Billboard Instance Bind Group"),
                layout: &self.particle_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.particle_buffer.as_entire_binding(),
                }],
            });
        }
        if !particle_items.is_empty() {
            frame.queue.write_buffer(
                &self.particle_buffer,
                0,
                bytemuck::cast_slice(
                    &particle_items
                        .iter()
                        .map(|item| item.gpu)
                        .collect::<Vec<_>>(),
                ),
            );
        }

        self.draws.clear();
        for batch in frame
            .batches
            .iter()
            .filter(|batch| batch.transparent_material.is_some())
        {
            let Some(material) = batch.transparent_material.clone() else {
                continue;
            };
            if material.validate().is_err() {
                continue;
            }
            for instance in &batch.instances {
                let model = Mat4::from_cols_array_2d(&instance.model_matrix);
                let depth = -(view * model * glam::Vec4::W).z;
                if depth.is_finite() {
                    self.draws.push(TransparentDrawItem {
                        entity_id: instance.entity_id,
                        depth,
                        mesh: batch.mesh.clone(),
                        model,
                        material: material.clone(),
                    });
                }
            }
        }
        Self::sort_draws(&mut self.draws);
        self.draw_order = sort_mixed_draw_order(
            &self.draws.iter().map(|draw| draw.depth).collect::<Vec<_>>(),
            &self.particle_depths,
        );

        for draw in &self.draws {
            let texture_id = draw
                .material
                .texture
                .as_ref()
                .map_or(0, |handle| handle.id());
            if !self.particle_texture_bind_groups.contains_key(&texture_id) {
                let texture = frame
                    .texture_cache
                    .get_or_upload_optional(draw.material.texture.clone(), frame.asset_manager);
                let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Transparent Mesh Texture Bind Group"),
                    layout: &self.particle_texture_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&texture.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&texture.sampler),
                        },
                    ],
                });
                self.particle_texture_bind_groups
                    .insert(texture_id, bind_group);
            }
        }

        let required = self.draws.len().max(1);
        if required > self.object_buffer_capacity {
            let new_capacity = required.next_power_of_two();
            let object_size = std::mem::size_of::<TransparentObjectUniform>() as u64;
            self.object_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Transparent Object Buffer"),
                size: object_size * new_capacity as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.object_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Transparent Object Bind Group"),
                layout: &self.object_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.object_buffer,
                        offset: 0,
                        size: Some(
                            std::num::NonZeroU64::new(object_size)
                                .unwrap_or(std::num::NonZeroU64::MIN),
                        ),
                    }),
                }],
            });
            self.object_buffer_capacity = new_capacity;
        }

        let object_data: Vec<_> = self
            .draws
            .iter()
            .map(|draw| TransparentObjectUniform {
                model: draw.model.to_cols_array_2d(),
                color: draw.material.base_color,
                alpha_cutoff: draw.material.alpha_cutoff.unwrap_or(0.0),
                cutoff_enabled: u32::from(draw.material.alpha_cutoff.is_some()),
                _padding: [0; 42],
            })
            .collect();
        if !object_data.is_empty() {
            frame
                .queue
                .write_buffer(&self.object_buffer, 0, bytemuck::cast_slice(&object_data));
        }
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        _surface_view: &wgpu::TextureView,
    ) {
        let (Some(target), Some(depth)) = (
            resources.get_if_handle(self.transparent_color_handle),
            resources.get_if_handle(self.depth_handle),
        ) else {
            return;
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Transparent Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: None,
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &self.view_proj_bind_group, &[]);
        let object_size = std::mem::size_of::<TransparentObjectUniform>() as u32;
        for source in &self.draw_order {
            match *source {
                TransparentDrawSource::Mesh(index) => {
                    let draw = &self.draws[index];
                    pass.set_pipeline(match draw.material.blend {
                        TransparentBlendMode::Alpha => &self.alpha_pipeline,
                        TransparentBlendMode::Additive => &self.additive_pipeline,
                    });
                    pass.set_bind_group(1, &self.object_bind_group, &[index as u32 * object_size]);
                    let texture_id = draw
                        .material
                        .texture
                        .as_ref()
                        .map_or(0, |handle| handle.id());
                    if let Some(texture_group) = self.particle_texture_bind_groups.get(&texture_id)
                    {
                        pass.set_bind_group(2, texture_group, &[]);
                    }
                    pass.set_vertex_buffer(0, draw.mesh.vertex_buffer.slice(..));
                    if let Some(index_buffer) = &draw.mesh.index_buffer {
                        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(
                            draw.mesh.index_offset..draw.mesh.index_offset + draw.mesh.index_count,
                            0,
                            0..1,
                        );
                    } else {
                        pass.draw(0..draw.mesh.vertex_count, 0..1);
                    }
                }
                TransparentDrawSource::Particle(index) => {
                    pass.set_pipeline(if self.particle_additive[index] {
                        &self.particle_additive_pipeline
                    } else {
                        &self.particle_alpha_pipeline
                    });
                    pass.set_bind_group(1, &self.particle_bind_group, &[]);
                    if let Some(texture_group) = self
                        .particle_texture_ids
                        .get(index)
                        .and_then(|id| self.particle_texture_bind_groups.get(id))
                    {
                        pass.set_bind_group(2, texture_group, &[]);
                    }
                    pass.draw(0..6, index as u32..index as u32 + 1);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{sort_mixed_draw_order, TransparentDrawSource, TransparentPass};
    use crate::renderer::resource::{GDepth, ResourceTag, SceneColor, TransparentColor};

    #[test]
    fn transparent_pass_declares_hdr_overlay_and_depth_test_input() {
        let signature = TransparentPass::signature_for_test();
        assert_eq!(signature.name, "Transparent");
        assert!(signature.reads.iter().any(|slot| slot.name == GDepth::NAME));
        assert!(signature
            .reads
            .iter()
            .any(|slot| slot.name == SceneColor::NAME));
        let output = signature
            .writes
            .iter()
            .find(|slot| slot.name == TransparentColor::NAME)
            .expect("transparent pass must own TransparentColor");
        assert_eq!(output.format, Some(wgpu::TextureFormat::Rgba16Float));
    }

    #[test]
    fn transparent_meshes_and_particles_share_one_far_to_near_order() {
        let ordered = sort_mixed_draw_order(&[2.0, 8.0], &[6.0, 1.0]);
        assert_eq!(
            ordered,
            vec![
                TransparentDrawSource::Mesh(1),
                TransparentDrawSource::Particle(0),
                TransparentDrawSource::Mesh(0),
                TransparentDrawSource::Particle(1),
            ]
        );
    }
}
