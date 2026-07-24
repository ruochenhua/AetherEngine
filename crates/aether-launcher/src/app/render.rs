//! Frame rendering for the launcher.

use super::{App, LauncherState};
use aether_engine::renderer::{
    extract::{extract_optional_pass_data, extract_render_batches},
    frame::{FrameConfig, RenderFrame},
    gizmo::{build_transform_gizmo, entity_transform},
};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, trace};

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
    let frame_start = Instant::now();

    // Frame counter for SSR temporal jitter (stays 0 under --freeze-time)
    let frame_index = if app.freeze_time {
        0u32
    } else {
        app.frame_counter
    };
    app.frame_counter = app.frame_counter.wrapping_add(1);

    let ctx = app.ctx.as_mut().unwrap();
    let scheduler = app.scheduler.as_mut().unwrap();
    let egui_renderer = app.egui_renderer.as_mut().unwrap();

    // Update egui textures before surface acquisition so that
    // the font atlas is allocated even if the surface is not
    // yet ready (Timeout/Occluded/Outdated on first frame).
    for (id, image_delta) in &textures_delta.set {
        egui_renderer.update_texture(&ctx.device, &ctx.queue, *id, image_delta);
    }
    // Free textures egui no longer references. Without this every image
    // texture handed to the UI would stay GPU-resident for the life of the
    // renderer (the current font-atlas-only UI never frees anything, but any
    // future `Image` widget would leak one texture each). egui guarantees
    // freed ids are not referenced by this frame's paint jobs, so freeing
    // them up-front alongside the updates is safe.
    for id in &textures_delta.free {
        egui_renderer.free_texture(id);
    }

    // Acquire surface
    let t_acquire_0 = Instant::now();
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
    let acquire_ms = t_acquire_0.elapsed().as_secs_f64() * 1000.0;

    // The 3D pipeline renders to an sRGB view so hardware gamma encoding is
    // applied, while egui renders to the default non-sRGB view.
    let target_view = output.texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("3D Render Target"),
        format: Some(ctx.render_target_format()),
        ..Default::default()
    });
    let egui_view = output.texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("egui Render Target"),
        ..Default::default()
    });

    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Encoder"),
        });

    let mut extract_ms = 0.0;
    let mut apply_ms = 0.0;
    let mut execute_ms = 0.0;

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

            // Extract phase: ECS World → GPU-ready batches and optional pass data
            let t_extract_0 = Instant::now();
            let batches = extract_render_batches(world);
            let optional = extract_optional_pass_data(world);
            extract_ms = t_extract_0.elapsed().as_secs_f64() * 1000.0;

            // Update shared terrain geometry cache when the scene contains terrain.
            if let Some(terrain) = optional.terrain.as_ref() {
                if app.terrain_geometry.is_none() {
                    app.terrain_geometry = Some(Arc::new(std::sync::RwLock::new(
                        aether_engine::terrain::TerrainGeometry::new(&ctx.device),
                    )));
                }
                if let Ok(mut geom) = app.terrain_geometry.as_ref().unwrap().write() {
                    geom.update(&ctx.device, &ctx.queue, &app.camera, aspect, terrain);
                }
            } else {
                app.terrain_geometry = None;
            }

            // Transform gizmo: build dynamic debug lines for the anchor
            // (most recently selected) entity of the current selection.
            let gizmo_lines = if let Some(transform) = app
                .selection
                .anchor()
                .and_then(|entity| entity_transform(world, entity))
            {
                build_transform_gizmo(&transform)
            } else {
                vec![]
            };

            // Build per-frame configuration channel — all controllable pass
            // parameters flow through RenderFrame::config.
            let frame_config = FrameConfig {
                ssao_enabled: app.ssao_enabled,
                shadow_enabled: app.shadow_enabled,
                ibl_enabled: app.ibl_enabled,
                ssao_radius: app.ssao_radius,
                ssao_bias: app.ssao_bias,
                ssao_intensity: app.ssao_intensity,
                debug_mode: app.debug_mode as u32,
                ssr_debug_mode: app.ssr_debug_mode,
                ssr_enabled: app.ssr_enabled,
                ssr_frame_index: frame_index,
                tone_mapping_mode: app.tone_mapping_mode,
                bloom_enabled: app.bloom_enabled,
                bloom_threshold: app.bloom_threshold,
                bloom_intensity: 1.0,
                bloom_composite_intensity: app.bloom_intensity,
                fxaa_enabled: app.fxaa_enabled,
                fxaa_quality: app.fxaa_quality,
                fxaa_edge_threshold: app.fxaa_edge_threshold,
                screen_width: ctx.config.width,
                screen_height: ctx.config.height,
                dynamic_lines: gizmo_lines,
            };

            // Build per-frame context — all passes extract
            // what they need via apply_frame.
            let frame = RenderFrame {
                batches: Arc::from(batches),
                camera: &app.camera,
                lighting,
                queue: &ctx.queue,
                aspect,
                delta_time: dt,
                optional: &optional,
                terrain_geometry: app.terrain_geometry.clone(),
                config: &frame_config,
                texture_cache: app.texture_cache.as_ref().unwrap(),
                asset_manager: &app.asset_manager,
            };
            debug!(
                "frame {} scheduler passes={} optional.terrain.is_some()={}",
                app.frame_count,
                scheduler.pass_names().join(","),
                optional.terrain.is_some()
            );
            let t_apply_0 = Instant::now();
            scheduler.apply_frame_all(&frame);
            apply_ms = t_apply_0.elapsed().as_secs_f64() * 1000.0;

            let t_execute_0 = Instant::now();
            scheduler.execute_all(&mut encoder, &target_view, &frame, app.gpu_timer.as_mut());
            execute_ms = t_execute_0.elapsed().as_secs_f64() * 1000.0;
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
                view: &egui_view,
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

    let submit_time = frame_start.elapsed();
    ctx.queue.submit(std::iter::once(encoder.finish()));
    let post_submit_time = frame_start.elapsed();
    output.present();
    let frame_time = frame_start.elapsed();
    app.input.end_frame();

    trace!(
        "frame_timing total_ms={:.2} acquire_ms={:.2} extract_ms={:.2} apply_ms={:.2} execute_ms={:.2} submit_to_present_ms={:.2}",
        frame_time.as_secs_f64() * 1000.0,
        acquire_ms,
        extract_ms,
        apply_ms,
        execute_ms,
        (post_submit_time - submit_time).as_secs_f64() * 1000.0,
    );

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
