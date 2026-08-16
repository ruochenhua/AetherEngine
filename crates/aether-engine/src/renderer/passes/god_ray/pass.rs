// God ray pass trait implementation.

use super::{GodRayPass, GodRayUniform};
use crate::renderer::frame::RenderFrame;
use crate::renderer::light::sun_direction_from_lighting;
use crate::renderer::pass::{InitContext, Pass, PassSignature};
use crate::renderer::resource::{GDepth, GodRayColor};
use crate::renderer::resource_table::ResourceTable;

impl Pass for GodRayPass {
    fn name(&self) -> &str {
        "GodRay"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("GodRay")
            .read::<GDepth>()
            .write::<GodRayColor>(wgpu::TextureFormat::Rgba16Float)
    }

    fn init(ctx: &InitContext) -> Self {
        Self::new(ctx.device)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.depth_handle = Some(resources.handle::<GDepth>());
        self.god_ray_color_handle = Some(resources.handle::<GodRayColor>());

        let depth_view = resources.get(self.depth_handle.unwrap());

        self.texture_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("God Ray Texture Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(depth_view),
            }],
        }));
    }

    fn should_run(&self, _frame: &RenderFrame) -> bool {
        self.has_god_ray
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        self.has_god_ray = false;
        if let Some(god_ray) = frame.optional.god_ray.clone() {
            self.has_god_ray = true;
            let proj = frame.camera.projection_matrix(frame.aspect);
            let view = frame.camera.view_matrix();
            let view_proj = proj * view;
            let inv_view_proj = view_proj.inverse();

            let sun_toward = sun_direction_from_lighting(frame.lighting);

            let cfg = &god_ray.config;
            let uniforms = GodRayUniform {
                view_proj,
                inv_view_proj,
                camera_pos: glam::Vec4::from((frame.camera.position, 0.0)),
                sun_direction: glam::Vec4::from((sun_toward, 0.0)),
                params: glam::Vec4::new(cfg.samples as f32, cfg.density, cfg.decay, cfg.weight),
                exposure: cfg.exposure,
                _pad: [0.0; 3],
            };

            frame
                .queue
                .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
        }
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        _surface_view: &wgpu::TextureView,
    ) {
        if !self.has_god_ray {
            return;
        }

        let god_ray_color_view = resources.get(self.god_ray_color_handle.unwrap());
        let texture_bg = self
            .texture_bind_group
            .as_ref()
            .expect("GodRayPass: resolve not called");

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("God Ray Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: god_ray_color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        pass.set_bind_group(1, texture_bg, &[]);
        pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
        pass.draw(0..self.quad_vertex_count, 0..1);
    }
}
