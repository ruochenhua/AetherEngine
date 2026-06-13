//! Bloom pass command recording.
//!
//! Records the extract, downsample, upsample, and composite render passes
//! into the provided command encoder.

use super::BloomPass;
use crate::renderer::resource_table::ResourceTable;

/// Record the bloom pass commands.
pub(super) fn execute(
    pass: &BloomPass,
    encoder: &mut wgpu::CommandEncoder,
    resources: &ResourceTable,
    _surface_view: &wgpu::TextureView,
) {
    let result_view = resources.get(pass.result_handle.unwrap());

    // When disabled, copy the post-process input straight through to the
    // bloom result so downstream ToneMappingPass sees the same data.
    if !pass.enabled {
        let input_tex = resources
            .texture(pass.input_handle.unwrap())
            .expect("BloomPass input texture missing");
        let result_tex = resources
            .texture(pass.result_handle.unwrap())
            .expect("BloomPass result texture missing");
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: input_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: result_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            input_tex.size(),
        );
        return;
    }

    // 1. Extract bright regions
    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Bloom Extract"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &pass.bright_texture.as_ref().unwrap().1,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        rpass.set_pipeline(&pass.extract_pipeline);
        rpass.set_bind_group(0, pass.extract_bg.as_ref().unwrap(), &[]);
        rpass.set_bind_group(1, &pass.uniform_bind_group, &[]);
        rpass.set_vertex_buffer(0, pass.quad_vertex_buffer.slice(..));
        rpass.draw(0..pass.quad_vertex_count, 0..1);
    }

    // 2-4. Downsample chain
    let downsample_targets = [
        (
            &pass.mip0.as_ref().unwrap().1,
            pass.downsample0_bg.as_ref().unwrap(),
            "Bloom Downsample 0",
        ),
        (
            &pass.mip1.as_ref().unwrap().1,
            pass.downsample1_bg.as_ref().unwrap(),
            "Bloom Downsample 1",
        ),
        (
            &pass.mip2.as_ref().unwrap().1,
            pass.downsample2_bg.as_ref().unwrap(),
            "Bloom Downsample 2",
        ),
    ];

    for (view, bg, label) in &downsample_targets {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        rpass.set_pipeline(&pass.downsample_pipeline);
        rpass.set_bind_group(0, *bg, &[]);
        rpass.set_vertex_buffer(0, pass.quad_vertex_buffer.slice(..));
        rpass.draw(0..pass.quad_vertex_count, 0..1);
    }

    // 5-6. Upsample chain with additive blending (Mip2→Mip1, Mip1→Mip0)
    let upsample_targets = [
        (
            &pass.mip1.as_ref().unwrap().1,
            pass.upsample0_bg.as_ref().unwrap(),
            "Bloom Upsample 0",
        ),
        (
            &pass.mip0.as_ref().unwrap().1,
            pass.upsample1_bg.as_ref().unwrap(),
            "Bloom Upsample 1",
        ),
    ];

    for (view, bg, label) in &upsample_targets {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
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
        rpass.set_pipeline(&pass.upsample_pipeline);
        rpass.set_bind_group(0, *bg, &[]);
        rpass.set_vertex_buffer(0, pass.quad_vertex_buffer.slice(..));
        rpass.draw(0..pass.quad_vertex_count, 0..1);
    }

    // 7. Final upsample: Mip0 → BloomTexture (Clear — first write)
    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Bloom Upsample 2"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &pass.bloom_texture.as_ref().unwrap().1,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        rpass.set_pipeline(&pass.upsample_pipeline);
        rpass.set_bind_group(0, pass.upsample2_bg.as_ref().unwrap(), &[]);
        rpass.set_vertex_buffer(0, pass.quad_vertex_buffer.slice(..));
        rpass.draw(0..pass.quad_vertex_count, 0..1);
    }

    // 8. Composite: HDR + Bloom → BloomResult
    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Bloom Composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: result_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        rpass.set_pipeline(&pass.composite_pipeline);
        rpass.set_bind_group(0, pass.composite_bg.as_ref().unwrap(), &[]);
        rpass.set_bind_group(1, &pass.uniform_bind_group, &[]);
        rpass.set_vertex_buffer(0, pass.quad_vertex_buffer.slice(..));
        rpass.draw(0..pass.quad_vertex_count, 0..1);
    }
}
