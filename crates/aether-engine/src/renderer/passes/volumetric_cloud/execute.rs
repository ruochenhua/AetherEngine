//! Render pass recording for the volumetric cloud pass.

use crate::renderer::resource_table::ResourceTable;

use super::VolumetricCloudPass;

impl VolumetricCloudPass {
    /// Record the cloud render pass into the given command encoder.
    pub(super) fn record_render_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
    ) {
        if !self.has_clouds {
            return;
        }

        let cloud_color_view = resources.get(self.cloud_color_handle.unwrap());
        let texture_bg = self
            .texture_bind_group
            .as_ref()
            .expect("VolumetricCloudPass: resolve not called");

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Volumetric Cloud Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: cloud_color_view,
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

        let Some(noise_bg) = self.noise_bind_group.as_ref() else {
            return;
        };
        pass.set_bind_group(2, noise_bg, &[]);
        pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
        pass.draw(0..self.quad_vertex_count, 0..1);
    }
}
