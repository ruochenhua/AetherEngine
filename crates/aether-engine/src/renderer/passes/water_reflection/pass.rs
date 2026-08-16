// Water reflection pass trait implementation.

use super::{ReflectionUniform, WaterReflectionPass};
use crate::asset::mesh::InstanceData;
use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{InitContext, Pass, PassSignature};
use crate::renderer::renderable::ObjectUniform;
use crate::renderer::resource::{WaterReflectionColor, WaterReflectionDepth};
use crate::renderer::resource_table::ResourceTable;
use crate::terrain::ChunkInstanceData;
use glam::{Mat4, Vec3};

impl Pass for WaterReflectionPass {
    fn name(&self) -> &str {
        "WaterReflection"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("WaterReflection")
            .write::<WaterReflectionColor>(wgpu::TextureFormat::Rgba16Float)
            .write::<WaterReflectionDepth>(wgpu::TextureFormat::Depth32Float)
    }

    fn init(ctx: &InitContext) -> Self {
        Self::new(ctx.device, ctx.queue)
    }

    fn resolve(&mut self, _device: &wgpu::Device, resources: &ResourceTable) {
        self.color_handle = Some(resources.handle::<WaterReflectionColor>());
        self.depth_handle = Some(resources.handle::<WaterReflectionDepth>());
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        if let Some(water) = frame.optional.water.clone() {
            self.has_water = true;
            self.reflection_enabled = water.config.reflection_enabled;
            self.reflection_level = water.config.level;
        } else {
            self.has_water = false;
            self.reflection_enabled = false;
            return;
        }

        if !self.reflection_enabled {
            return;
        }

        self.batches = frame.batches.clone();

        // Mirror the camera across the water plane by post-multiplying the view
        // matrix with a Y-reflection matrix. This is more robust than manually
        // adjusting position/pitch.
        let level = self.reflection_level;
        let reflection_matrix = Mat4::from_cols_array_2d(&[
            [1.0, 0.0, 0.0, 0.0],
            [0.0, -1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 2.0 * level, 0.0, 1.0],
        ]);

        self.view = frame.camera.view_matrix() * reflection_matrix;
        self.proj = frame.camera.projection_matrix(frame.aspect);

        let light = &frame.lighting.light;
        self.light_dir = Vec3::new(
            -light.direction[0],
            -light.direction[1],
            -light.direction[2],
        );
        self.light_color = Vec3::new(
            light.color[0] * light.intensity,
            light.color[1] * light.intensity,
            light.color[2] * light.intensity,
        );
        self.ambient = Vec3::splat(frame.lighting.ambient_intensity);

        frame.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[ReflectionUniform {
                view: self.view.to_cols_array_2d(),
                proj: self.proj.to_cols_array_2d(),
                light_dir: self.light_dir.extend(0.0).to_array(),
                light_color: self.light_color.extend(0.0).to_array(),
                ambient: self.ambient.extend(0.0).to_array(),
            }]),
        );

        let obj_size = std::mem::size_of::<ObjectUniform>() as wgpu::BufferAddress;
        let batch_count = self.batches.len();
        if batch_count > self.object_buffer_capacity {
            let new_capacity = batch_count.max(256);
            self.object_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("WaterReflection Obj Buf"),
                size: (new_capacity as u64) * obj_size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.object_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("WaterReflection Obj BG"),
                layout: &self.pipeline.get_bind_group_layout(1),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.object_buffer,
                        offset: 0,
                        size: Some(std::num::NonZeroU64::new(obj_size).unwrap()),
                    }),
                }],
            });
            self.object_buffer_capacity = new_capacity;
        }
        let mut obj_data: Vec<u8> = Vec::with_capacity(batch_count * obj_size as usize);
        for batch in self.batches.iter() {
            let obj = ObjectUniform {
                albedo: batch.material.albedo,
                roughness: batch.material.roughness,
                metallic: batch.material.metallic,
            };
            obj_data.extend_from_slice(bytemuck::cast_slice(&[obj]));
        }
        if !obj_data.is_empty() {
            frame.queue.write_buffer(&self.object_buffer, 0, &obj_data);
        }

        let total_instances: usize = self.batches.iter().map(|b| b.instances.len()).sum();
        if total_instances > self.instance_buffer_capacity {
            let new_capacity = total_instances.max(256);
            self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("WaterReflection Instance Buf"),
                size: (new_capacity * std::mem::size_of::<InstanceData>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instance_buffer_capacity = new_capacity;
        }
        let mut instance_data: Vec<u8> =
            Vec::with_capacity(total_instances * std::mem::size_of::<InstanceData>());
        for batch in self.batches.iter() {
            instance_data.extend_from_slice(bytemuck::cast_slice(&batch.instances));
        }
        if !instance_data.is_empty() {
            frame
                .queue
                .write_buffer(&self.instance_buffer, 0, &instance_data);
        }

        self.texture_bind_groups.clear();
        for batch in self.batches.iter() {
            let gpu_tex = frame
                .texture_cache
                .get_or_upload_optional(batch.albedo_texture.clone(), frame.asset_manager);
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("WaterReflection Texture BG"),
                layout: &self.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&gpu_tex.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&gpu_tex.sampler),
                    },
                ],
            });
            self.texture_bind_groups.push(bg);
        }

        // Update terrain material for reflection rendering.
        self.terrain_geometry = frame.terrain_geometry.clone();
        if let Some(terrain) = frame.optional.terrain.as_ref() {
            self.update_terrain_material(
                terrain,
                frame.queue,
                frame.texture_cache,
                frame.asset_manager,
            );
        }
    }

    fn should_run(&self, _frame: &RenderFrame) -> bool {
        self.has_water && self.reflection_enabled
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        _surface_view: &wgpu::TextureView,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("WaterReflection"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: resources.get(self.color_handle.unwrap()),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: resources.get(self.depth_handle.unwrap()),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.uniform_bind_group, &[]);

        let obj_size = std::mem::size_of::<ObjectUniform>() as wgpu::BufferAddress;
        let mut instance_offset = 0usize;
        for (batch_index, batch) in self.batches.iter().enumerate() {
            let instance_count = batch.instances.len() as u32;
            if instance_count == 0 || batch.mesh.vertex_count == 0 {
                continue;
            }

            let offset = batch_index as u32 * obj_size as u32;
            pass.set_bind_group(1, &self.object_bind_group, &[offset]);
            pass.set_bind_group(2, &self.texture_bind_groups[batch_index], &[]);

            pass.set_vertex_buffer(0, batch.mesh.vertex_buffer.slice(..));
            let instance_byte_start =
                (instance_offset * std::mem::size_of::<InstanceData>()) as wgpu::BufferAddress;
            let instance_byte_end = instance_byte_start
                + (batch.instances.len() * std::mem::size_of::<InstanceData>())
                    as wgpu::BufferAddress;
            pass.set_vertex_buffer(
                1,
                self.instance_buffer
                    .slice(instance_byte_start..instance_byte_end),
            );
            if let Some(ref ib) = batch.mesh.index_buffer {
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                let start = batch.mesh.index_offset;
                let end = start + batch.mesh.index_count;
                pass.draw_indexed(start..end, 0, 0..instance_count);
            } else {
                pass.draw(0..batch.mesh.vertex_count, 0..instance_count);
            }
            instance_offset += batch.instances.len();
        }

        // Render terrain into the reflection so shorelines/hills appear on the water.
        if let Some(terrain_geometry) = &self.terrain_geometry {
            let terrain = terrain_geometry.read().unwrap();
            let chunks = terrain.chunks();
            if !chunks.is_empty() {
                pass.set_pipeline(&self.terrain_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_bind_group(1, &self.terrain_bind_group, &[]);
                pass.set_vertex_buffer(1, terrain.instance_buffer().slice(..));
                for (chunk_index, chunk) in chunks.iter().enumerate() {
                    let lod_mesh = &terrain.chunk_meshes()[chunk_index][chunk.lod as usize];
                    let instance_start = (chunk_index * std::mem::size_of::<ChunkInstanceData>())
                        as wgpu::BufferAddress;
                    let instance_end = instance_start
                        + std::mem::size_of::<ChunkInstanceData>() as wgpu::BufferAddress;
                    pass.set_vertex_buffer(0, lod_mesh.vertex_buffer.slice(..));
                    pass.set_vertex_buffer(
                        1,
                        terrain
                            .instance_buffer()
                            .slice(instance_start..instance_end),
                    );
                    if let Some(ref ib) = lod_mesh.index_buffer {
                        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..lod_mesh.index_count, 0, 0..1);
                    } else {
                        pass.draw(0..lod_mesh.vertex_count, 0..1);
                    }
                }
            }
        }
    }
}
