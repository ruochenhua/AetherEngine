//! Water pass command recording.
//!
//! Records the water render pass into the provided command encoder.

use super::WaterPass;
use crate::renderer::resource_table::ResourceTable;

/// Record the water render pass commands.
pub(super) fn execute(
    pass: &WaterPass,
    encoder: &mut wgpu::CommandEncoder,
    resources: &ResourceTable,
    _surface_view: &wgpu::TextureView,
) {
    if !pass.has_water {
        return;
    }

    let water_color_view = resources.get(pass.water_color_handle.unwrap());
    let texture_bg = pass
        .texture_bind_group
        .as_ref()
        .expect("WaterPass: resolve not called");

    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Water Pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: water_color_view,
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

    rpass.set_pipeline(&pass.pipeline);
    rpass.set_bind_group(0, &pass.uniform_bind_group, &[]);
    rpass.set_bind_group(1, texture_bg, &[]);
    rpass.set_bind_group(
        2,
        pass.water_texture_bind_group
            .as_ref()
            .expect("WaterPass: water texture bind group not initialized"),
        &[],
    );
    rpass.set_vertex_buffer(0, pass.mesh.vertex_buffer.slice(..));
    if let Some(ref index_buffer) = pass.mesh.index_buffer {
        rpass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        rpass.draw_indexed(0..pass.mesh.index_count, 0, 0..1);
    } else {
        rpass.draw(0..pass.mesh.vertex_count, 0..1);
    }
}
