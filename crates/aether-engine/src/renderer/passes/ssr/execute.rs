//! SSR command recording.
//!
//! Records the half-resolution trace pass and the full-resolution bilateral
//! upsample pass into the provided command encoder.

use super::SSRPass;
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;

/// Record the SSR trace and upsample passes into `encoder`.
pub(super) fn execute(
    pass: &SSRPass,
    encoder: &mut wgpu::CommandEncoder,
    resources: &ResourceTable,
    _surface_view: &wgpu::TextureView,
) {
    let ssr_trace_view = resources.get(resources.handle::<SsrTraceResult>());

    // Stage 1: Trace at half resolution (uses trace_bind_group WITHOUT SsrTraceResult)
    let trace_bg = pass
        .trace_bind_group
        .as_ref()
        .expect("SSR: trace resolve not called");
    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SSR Trace (Half-Res)"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ssr_trace_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        rpass.set_pipeline(&pass.trace_pipeline);
        rpass.set_bind_group(0, trace_bg, &[]);
        rpass.set_bind_group(1, &pass.settings_bind_group, &[]);
        rpass.set_vertex_buffer(0, pass.quad_vertex_buffer.slice(..));
        rpass.draw(0..pass.quad_vertex_count, 0..1);
    }

    // Stage 2: Bilateral upsample (uses upsample_bind_group WITH SsrTraceResult)
    let up_bg = pass
        .upsample_bind_group
        .as_ref()
        .expect("SSR: upsample resolve not called");
    let reflection_view = resources.get(resources.handle::<ReflectionTexture>());
    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SSR Upsample (Full-Res)"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: reflection_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        rpass.set_pipeline(&pass.upsample_pipeline);
        rpass.set_bind_group(0, up_bg, &[]);
        rpass.set_bind_group(1, &pass.settings_bind_group, &[]);
        rpass.set_vertex_buffer(0, pass.quad_vertex_buffer.slice(..));
        rpass.draw(0..pass.quad_vertex_count, 0..1);
    }
}
