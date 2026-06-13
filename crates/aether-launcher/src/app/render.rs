//! Frame rendering for the launcher.

use super::{App, LauncherState};
use aether_engine::renderer::{
    extract::extract_render_batches,
    frame::RenderFrame,
    gizmo::{build_transform_gizmo, selected_entity_transform},
};
use std::sync::Arc;
use tracing::{error, info};

/// Save a GPU screenshot buffer to a PNG file on disk.
fn save_screenshot(
    device: &wgpu::Device,
    buffer: &wgpu::Buffer,
    width: u32,
    height: u32,
    bytes_per_row: u32,
    path: &std::path::Path,
) -> anyhow::Result<()> {
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());

    let data = buffer.slice(..).get_mapped_range();

    // BGRA -> RGBA and remove row padding
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let row_start = (y * bytes_per_row) as usize;
        for x in 0..width {
            let idx = row_start + (x * 4) as usize;
            rgba.push(data[idx + 2]); // R
            rgba.push(data[idx + 1]); // G
            rgba.push(data[idx]); // B
            rgba.push(data[idx + 3]); // A
        }
    }
    drop(data);
    buffer.unmap();

    let img = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| anyhow::anyhow!("Failed to create image buffer"))?;
    img.save(path)?;

    Ok(())
}

/// Execute one full render frame.
pub(crate) fn frame(
    app: &mut App,
    event_loop: &winit::event_loop::ActiveEventLoop,
    paint_jobs: Vec<egui::ClippedPrimitive>,
    textures_delta: egui::TexturesDelta,
    screen_descriptor: egui_wgpu::ScreenDescriptor,
    should_screenshot: bool,
    dt: f32,
) {
    let ctx = app.ctx.as_mut().unwrap();
    let scheduler = app.scheduler.as_mut().unwrap();
    let egui_renderer = app.egui_renderer.as_mut().unwrap();

    // Update egui textures before surface acquisition so that
    // the font atlas is allocated even if the surface is not
    // yet ready (Timeout/Occluded/Outdated on first frame).
    for (id, image_delta) in &textures_delta.set {
        egui_renderer.update_texture(&ctx.device, &ctx.queue, *id, image_delta);
    }

    // Acquire surface
    let output = match ctx.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(o) => o,
        wgpu::CurrentSurfaceTexture::Suboptimal(o) => o,
        wgpu::CurrentSurfaceTexture::Lost => {
            ctx.resize(ctx.config.width, ctx.config.height);
            return;
        }
        wgpu::CurrentSurfaceTexture::Validation => {
            event_loop.exit();
            return;
        }
        wgpu::CurrentSurfaceTexture::Timeout
        | wgpu::CurrentSurfaceTexture::Occluded
        | wgpu::CurrentSurfaceTexture::Outdated => {
            return;
        }
    };
    let target_view = output
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Encoder"),
        });

    match &mut app.state {
        LauncherState::Menu => {
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Menu"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.08,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                multiview_mask: None,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        LauncherState::Running {
            ref world,
            ref mut lighting,
        } => {
            let aspect = ctx.config.width as f32 / ctx.config.height as f32;

            // Update lighting from ECS (camera position + light data)
            lighting.camera_pos = app.camera.position.to_array();
            if let Some((transform, light)) = world
                .query::<(
                    &aether_engine::ecs::components::Transform,
                    &aether_engine::ecs::components::Light,
                )>()
                .iter()
                .next()
            {
                lighting.light.direction = (transform.rotation * glam::Vec3::NEG_Y)
                    .normalize()
                    .to_array();
                lighting.light.color = light.color;
                lighting.light.intensity = light.intensity;
            }

            // Extract phase: ECS World → GPU-ready batches
            let batches = extract_render_batches(world);

            // Transform gizmo: build dynamic debug lines for selected entity
            let gizmo_lines = if let Some((_, transform)) = selected_entity_transform(world) {
                build_transform_gizmo(&transform)
            } else {
                vec![]
            };
            scheduler.set_dynamic_lines(gizmo_lines);

            // Build per-frame context — all passes extract
            // what they need via apply_frame.
            let frame = RenderFrame {
                batches: Arc::from(batches),
                camera: &app.camera,
                lighting,
                queue: &ctx.queue,
                aspect,
                delta_time: dt,
                world,
            };
            scheduler.set_feature_flags(app.ssao_enabled, app.shadow_enabled, app.ibl_enabled);
            scheduler.set_ssao_params(app.ssao_radius, app.ssao_bias, app.ssao_intensity);
            scheduler.set_debug_mode(app.debug_mode as u32);
            scheduler.set_ssr_debug_mode(app.ssr_debug_mode);
            scheduler.set_ssr_enabled(app.ssr_enabled);
            scheduler.set_tone_mapping_mode(app.tone_mapping_mode, &ctx.queue);
            scheduler.set_bloom_params(
                app.bloom_enabled,
                app.bloom_threshold,
                1.0,
                app.bloom_intensity,
                &ctx.queue,
            );
            scheduler.set_fxaa_params(
                app.fxaa_enabled,
                app.fxaa_quality,
                app.fxaa_edge_threshold,
                &ctx.queue,
            );
            scheduler.apply_frame_all(&frame);
            scheduler.execute_all(&mut encoder, &target_view, &frame, app.gpu_timer.as_mut());
        }
    }

    // Screenshot copy before egui pass
    if should_screenshot {
        let (size, bpr) =
            crate::pipeline::screenshot_buffer_size(ctx.config.width, ctx.config.height);
        app.screenshot_bytes_per_row = bpr;
        let buf = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Screenshot Buffer"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &output.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bpr),
                    rows_per_image: Some(ctx.config.height),
                },
            },
            wgpu::Extent3d {
                width: ctx.config.width,
                height: ctx.config.height,
                depth_or_array_layers: 1,
            },
        );
        app.screenshot_buffer = Some(buf);
    }

    // egui pass
    egui_renderer.update_buffers(
        &ctx.device,
        &ctx.queue,
        &mut encoder,
        &paint_jobs,
        &screen_descriptor,
    );
    {
        let rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("egui"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            multiview_mask: None,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        egui_renderer.render(&mut rp.forget_lifetime(), &paint_jobs, &screen_descriptor);
    }

    ctx.queue.submit(std::iter::once(encoder.finish()));
    output.present();
    app.input.end_frame();

    if should_screenshot {
        if let Some(buf) = app.screenshot_buffer.take() {
            if let Some(path) = app.pending_screenshot_path.take() {
                if let Err(e) = save_screenshot(
                    &ctx.device,
                    &buf,
                    ctx.config.width,
                    ctx.config.height,
                    app.screenshot_bytes_per_row,
                    &path,
                ) {
                    error!("Screenshot failed: {:?}", e);
                } else {
                    info!("Screenshot saved to {:?}", path);
                }
                app.screenshot_taken = true;
            }
        }
    }

    if app.exit_after_frames.is_some_and(|n| app.frame_count >= n) {
        event_loop.exit();
    }
}
