// Atmosphere pass trait implementation.

use super::{AtmospherePass, AtmosphereUniform};
use crate::renderer::frame::RenderFrame;
use crate::renderer::light::sun_direction_from_lighting;
use crate::renderer::pass::{InitContext, Pass, PassSignature};
use crate::renderer::resource::{GDepth, SceneColor};
use crate::renderer::resource_table::ResourceTable;

impl Pass for AtmospherePass {
    fn name(&self) -> &str {
        "Atmosphere"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("Atmosphere")
            .read::<GDepth>()
            .write::<SceneColor>(wgpu::TextureFormat::Rgba16Float)
    }

    fn init(ctx: &InitContext) -> Self {
        Self::new(ctx.device)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.scene_color_handle = Some(resources.handle::<SceneColor>());
        self.depth_handle = Some(resources.handle::<GDepth>());

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Atmosphere Texture Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                }],
            });

        self.texture_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Atmosphere Texture Bind Group"),
            layout: &texture_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    resources.get(self.depth_handle.unwrap()),
                ),
            }],
        }));
    }

    fn should_run(&self, _frame: &RenderFrame) -> bool {
        self.has_atmosphere
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        if let Some(atmos) = frame.optional.atmosphere.clone() {
            self.has_atmosphere = true;
            let sun_toward = sun_direction_from_lighting(frame.lighting);

            let proj = frame.camera.projection_matrix(frame.aspect);
            let view = frame.camera.view_matrix();
            let inv_view_proj = (proj * view).inverse();

            let uniforms = AtmosphereUniform {
                sun_direction: sun_toward.into(),
                camera_pos: frame.camera.position.into(),
                planet_radius: atmos.config.planet_radius,
                atmosphere_height: atmos.config.atmosphere_height,
                rayleigh_scale_height: atmos.config.rayleigh_scale_height,
                mie_scale_height: atmos.config.mie_scale_height,
                rayleigh_scattering: atmos.config.rayleigh_scattering,
                mie_scattering: atmos.config.mie_scattering,
                sun_intensity: atmos.config.sun_intensity,
                mie_asymmetry: atmos.config.mie_asymmetry,
                ozone_absorption: atmos.config.ozone_absorption,
                ozone_scale_height: atmos.config.ozone_scale_height,
                multi_scattering_factor: atmos.config.multi_scattering_factor,
                inv_view_proj: inv_view_proj.to_cols_array_2d(),
                ..Default::default()
            };
            frame
                .queue
                .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
        } else {
            self.has_atmosphere = false;
        }
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        _surface_view: &wgpu::TextureView,
    ) {
        if !self.has_atmosphere {
            return;
        }

        let scene_color_view = resources.get(self.scene_color_handle.unwrap());
        let texture_bg = self
            .texture_bind_group
            .as_ref()
            .expect("AtmospherePass: resolve not called");

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Atmosphere Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: scene_color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
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
