
use super::*;
use crate::ecs::components::GodRay;
use crate::ecs::World;
use crate::renderer::extract::extract_optional_pass_data;
use crate::renderer::frame::FrameConfig;
use crate::renderer::frame::RenderFrame;
use crate::renderer::light::{sun_direction_from_lighting, LightingUniforms};
use crate::renderer::pass::Pass;
use crate::renderer::resource_table::ResourceTable;
use glam::Vec3;
use wgpu::util::DeviceExt;

fn headless_device() -> wgpu::Device {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("need adapter");
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .expect("need device")
        .0
}

#[test]
fn god_ray_pass_signature_reads_depth_and_writes_overlay() {
    let device = headless_device();
    let sig = GodRayPass::new(&device).signature();
    assert_eq!(sig.name, "GodRay");
    assert!(sig.reads.iter().any(|s| s.name == "gbuffer_depth"));
    assert!(sig.writes.iter().any(|s| s.name == "god_ray_color"));
}

#[test]
fn god_ray_pass_skipped_without_component() {
    let device = headless_device();
    let pass = GodRayPass::new(&device);
    assert!(!pass.has_god_ray);
}

#[test]
fn god_ray_uniform_default_is_aligned() {
    let _ = GodRayUniform::default();
    assert_eq!(std::mem::size_of::<GodRayUniform>() % 16, 0);
}

/// Verifies that the sun direction written to the GodRay uniform buffer
/// matches the shared `sun_direction_from_lighting` helper.
#[test]
fn god_ray_pass_uses_light_direction_for_sun() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("need adapter");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("need device");

    let mut pass = GodRayPass::new(&device);
    let mut world = World::new();
    world.spawn((GodRay {
        config: crate::scene::GodRayConfig {
            samples: 16,
            density: 0.6,
            decay: 0.9,
            weight: 0.3,
            exposure: 0.2,
        },
    },));
    let optional = extract_optional_pass_data(&world);

    let camera = crate::renderer::camera::FlyCamera::default();
    let lighting = LightingUniforms::default();
    let assets = crate::asset::AssetManager::new();
    let texture_cache = crate::asset::texture_cache::GpuTextureCache::new(&device, &queue);
    let frame = crate::renderer::frame::RenderFrame {
        batches: std::sync::Arc::from([]),
        camera: &camera,
        lighting: &lighting,
        queue: &queue,
        aspect: 1.0,
        delta_time: 0.016,
        config: &FrameConfig::default(),
        optional: &optional,
        terrain_geometry: None,
        texture_cache: &texture_cache,
        asset_manager: &assets,
    };

    pass.apply_frame(&frame);
    assert!(pass.has_god_ray);

    let uniform_size = std::mem::size_of::<GodRayUniform>() as u64;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("GodRay Sun Direction Readback"),
        size: uniform_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("GodRay Sun Direction Copy"),
    });
    encoder.copy_buffer_to_buffer(&pass.uniform_buffer, 0, &staging, 0, uniform_size);
    queue.submit(std::iter::once(encoder.finish()));

    let slice = staging.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| {
        result.expect("failed to map uniform readback buffer");
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

    let data = slice.get_mapped_range();
    let uniforms: &[GodRayUniform] = bytemuck::cast_slice(&data);
    let expected_sun = sun_direction_from_lighting(&lighting);
    let actual_sun = Vec3::new(
        uniforms[0].sun_direction.x,
        uniforms[0].sun_direction.y,
        uniforms[0].sun_direction.z,
    );
    assert!(
        actual_sun.abs_diff_eq(expected_sun, 1e-4),
        "expected sun_direction {expected_sun:?}, got {actual_sun:?}"
    );
}

/// Headless render-to-texture test that verifies the god ray pass produces
/// a visible light-shaft overlay and saves the result as a PNG for visual
/// inspection.
#[test]
fn god_ray_pass_renders_overlay_to_texture() {
    use std::any::TypeId;
    use std::borrow::Cow;

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("need adapter");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("need device");

    let width = 128u32;
    let height = 128u32;

    // Depth texture: far plane (sky) everywhere so the ray march accumulates light.
    let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("GodRay Test Depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

    // Clear the depth texture to the far plane (sky) so the ray march accumulates light.
    let mut clear_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("GodRay Test Clear Depth"),
    });
    clear_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("GodRay Test Clear Depth Pass"),
        color_attachments: &[],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: &depth_view,
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
    queue.submit(std::iter::once(clear_encoder.finish()));

    // Draw a small centered occluder so the god ray test image shows visible shafts.
    let occluder_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("GodRay Test Occluder Shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(
            r#"
@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(pos, 0.5, 1.0);
}
"#,
        )),
    });
    let occluder_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("GodRay Test Occluder Layout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    let occluder_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("GodRay Test Occluder Pipeline"),
        layout: Some(&occluder_layout),
        vertex: wgpu::VertexState {
            module: &occluder_shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                }],
            }],
        },
        fragment: None,
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let occluder_vertices: [[f32; 2]; 6] = [
        [-0.25, -0.25],
        [0.25, -0.25],
        [0.25, 0.25],
        [-0.25, -0.25],
        [0.25, 0.25],
        [-0.25, 0.25],
    ];
    let occluder_vbo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("GodRay Test Occluder Vtx"),
        contents: bytemuck::cast_slice(&occluder_vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let mut occ_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("GodRay Test Occluder"),
    });
    {
        let mut pass = occ_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("GodRay Test Occluder Pass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&occluder_pipeline);
        pass.set_vertex_buffer(0, occluder_vbo.slice(..));
        pass.draw(0..6, 0..1);
    }
    queue.submit(std::iter::once(occ_encoder.finish()));

    // God ray output texture.
    let output_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("GodRay Test Output"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let mut resources = ResourceTable::new();
    resources.allocate_with_texture(
        TypeId::of::<GDepth>(),
        "gbuffer_depth",
        depth_texture,
        depth_view,
    );
    resources.allocate_with_texture(
        TypeId::of::<GodRayColor>(),
        "god_ray_color",
        output_texture,
        output_view,
    );

    let mut pass = GodRayPass::new(&device);
    pass.resolve(&device, &resources);

    // World with a directional light and a god-ray component.
    let mut world = World::new();
    world.spawn((
        crate::ecs::components::Transform {
            translation: glam::Vec3::new(0.0, 10.0, 0.0),
            rotation: glam::Quat::from_euler(
                glam::EulerRot::YXZ,
                0.0,
                std::f32::consts::FRAC_PI_4,
                0.0,
            ),
            scale: glam::Vec3::ONE,
        },
        crate::ecs::components::Light {
            light_type: crate::renderer::light::LightType::Directional,
            color: [1.0, 0.95, 0.8],
            intensity: 1.0,
            cast_shadow: false,
        },
    ));
    world.spawn((GodRay {
        config: crate::scene::GodRayConfig {
            samples: 16,
            density: 0.6,
            decay: 0.9,
            weight: 0.3,
            exposure: 0.2,
        },
    },));

    let camera = crate::renderer::camera::FlyCamera {
        position: glam::Vec3::new(0.0, 1.0, 5.0),
        yaw: -std::f32::consts::FRAC_PI_2,
        pitch: 0.0,
        fov: 45.0f32.to_radians(),
        near: 0.1,
        far: 100.0,
        speed: 1.0,
        base_speed: 1.0,
        min_speed: 0.1,
        max_speed: 10.0,
        sensitivity: 0.001,
        active: false,
    };
    let lighting = crate::renderer::light::LightingUniforms::default();

    let optional = extract_optional_pass_data(&world);
    let texture_cache = crate::asset::texture_cache::GpuTextureCache::new(&device, &queue);
    let asset_manager = crate::asset::AssetManager::new();
    let frame = RenderFrame {
        batches: std::sync::Arc::from([]),
        camera: &camera,
        lighting: &lighting,
        queue: &queue,
        aspect: width as f32 / height as f32,
        delta_time: 0.016,
        config: &FrameConfig::default(),
        optional: &optional,
        terrain_geometry: None,
        texture_cache: &texture_cache,
        asset_manager: &asset_manager,
    };
    pass.apply_frame(&frame);
    assert!(pass.has_god_ray);

    let output_view_ref = resources.get(pass.god_ray_color_handle.unwrap());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("GodRay Test"),
    });
    pass.execute(&mut encoder, &resources, output_view_ref);
    queue.submit(std::iter::once(encoder.finish()));

    // Read back the output texture.
    let bytes_per_row = (width * 8).div_ceil(256) * 256;
    let buffer_size = bytes_per_row as u64 * height as u64;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("GodRay Test Readback"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("GodRay Test Copy"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: resources
                .texture(pass.god_ray_color_handle.unwrap())
                .unwrap(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(std::iter::once(encoder.finish()));

    readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

    let data = readback.slice(..).get_mapped_range();
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let row_start = (y * bytes_per_row) as usize;
        for x in 0..width {
            let idx = row_start + (x * 8) as usize;
            let r = half::f16::from_ne_bytes([data[idx], data[idx + 1]]).to_f32();
            let g = half::f16::from_ne_bytes([data[idx + 2], data[idx + 3]]).to_f32();
            let b = half::f16::from_ne_bytes([data[idx + 4], data[idx + 5]]).to_f32();
            let a = half::f16::from_ne_bytes([data[idx + 6], data[idx + 7]]).to_f32();
            rgba.push((r.clamp(0.0, 1.0) * 255.0) as u8);
            rgba.push((g.clamp(0.0, 1.0) * 255.0) as u8);
            rgba.push((b.clamp(0.0, 1.0) * 255.0) as u8);
            rgba.push((a.clamp(0.0, 1.0) * 255.0) as u8);
        }
    }
    drop(data);
    readback.unmap();

    let img = image::RgbaImage::from_raw(width, height, rgba).expect("valid image buffer");
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let out_path = workspace_root.join("tests/output/14_god_rays_unit.png");
    std::fs::create_dir_all(out_path.parent().unwrap()).ok();
    img.save(&out_path).expect("save screenshot");

    // Sanity check: the overlay is not entirely transparent.
    let avg_alpha: f32 = img.pixels().map(|p| p[3] as f32).sum::<f32>() / (width * height) as f32;
    assert!(
        avg_alpha > 1.0,
        "god ray overlay should be visible (avg alpha {:.2})",
        avg_alpha
    );
}
