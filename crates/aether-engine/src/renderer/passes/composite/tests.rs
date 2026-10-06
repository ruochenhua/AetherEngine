use super::*;
use crate::renderer::pass::Pass;
use crate::renderer::resource::{
    CloudColor, GAlbedo, GMaterial, GNormal, GPosition, GodRayColor, PostProcessInput,
    ReflectionTexture, ResourceTag, SceneColor, TransparentColor, WaterColor,
};
use crate::renderer::resource_table::ResourceTable;
use std::any::TypeId;
fn headless_device() -> wgpu::Device {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("need adapter");
    let (device, _) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("need device");
    device
}

#[test]
fn signature_declares_reads_and_write() {
    let device = headless_device();
    let sig = CompositePass::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb).signature();
    assert_eq!(sig.name, "Composite");
    assert_eq!(sig.reads.len(), 9);
    assert_eq!(sig.writes.len(), 1);
}

#[test]
fn transparent_variant_declares_its_overlay_read_only_when_enabled() {
    let device = headless_device();
    let off = CompositePass::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb).signature();
    let on = CompositePass::new_with_transparency(&device, wgpu::TextureFormat::Bgra8UnormSrgb)
        .signature();

    assert_eq!(off.reads.len(), 9);
    assert!(!off
        .reads
        .iter()
        .any(|slot| slot.name == TransparentColor::NAME));
    assert_eq!(on.reads.len(), 10);
    assert!(on
        .reads
        .iter()
        .any(|slot| slot.name == TransparentColor::NAME));
}

#[test]
fn transparent_shader_variant_composites_premultiplied_overlay() {
    let source = shaders::shader_source(true);
    assert!(source.contains("@binding(10) var transparent_color"));
    assert!(source.contains("final_color = final_color * (1.0 - transparent.a) + transparent.rgb"));
    let feature_off = shaders::shader_source(false);
    assert!(!feature_off.contains("@binding(10) var transparent_color"));
    assert!(!feature_off.contains("textureSample(transparent_color"));
}

#[test]
fn transparent_then_water_uses_the_frozen_composite_equation() {
    let result = super::compose_overlays(
        [0.2, 0.4, 0.8],
        [0.5, 0.0, 0.0, 0.5],
        [0.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        [0.1, 0.3, 0.5, 0.25],
    );
    for (actual, expected) in result.into_iter().zip([0.475, 0.225, 0.425]) {
        assert!((actual - expected).abs() < 1e-6);
    }
}

#[test]
fn transparent_additive_and_empty_overlays_match_the_frozen_sentinels() {
    let additive = super::compose_overlays(
        [0.2, 0.4, 0.8],
        [0.0, 0.25, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
    );
    let empty = super::compose_overlays(
        [0.2, 0.4, 0.8],
        [0.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
    );

    for (actual, expected) in additive.into_iter().zip([0.2, 0.65, 0.8]) {
        assert!((actual - expected).abs() < 1e-6);
    }
    for (actual, expected) in empty.into_iter().zip([0.2, 0.4, 0.8]) {
        assert!((actual - expected).abs() < 1e-6);
    }
}

#[test]
fn transparent_shader_applies_overlay_before_cloud_god_ray_and_water() {
    let source = shaders::shader_source(true);
    let transparent = source
        .find("let transparent = textureSample")
        .expect("transparent sample must exist");
    let cloud = source
        .find("let with_clouds")
        .expect("cloud stage must exist");
    let god_ray = source
        .find("let with_god_rays")
        .expect("god-ray stage must exist");
    let water = source
        .find("if (water.a > 0.0001)")
        .expect("water stage must exist");
    assert!(transparent < cloud && cloud < god_ray && god_ray < water);
    assert_eq!(
        COMPOSITE_OVERLAY_ORDER,
        [
            "TransparentColor",
            "CloudColor",
            "GodRayColor",
            "WaterColor"
        ]
    );
}

#[test]
fn init_creates_resources() {
    let _pass = CompositePass::new(&headless_device(), wgpu::TextureFormat::Bgra8UnormSrgb);
}

fn half_pixel_bytes(pixels: &[[f32; 4]]) -> Vec<u8> {
    pixels
        .iter()
        .flat_map(|pixel| {
            pixel
                .iter()
                .flat_map(|value| half::f16::from_f32(*value).to_le_bytes())
        })
        .collect()
}

fn allocate_input<T: ResourceTag>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    table: &mut ResourceTable,
    pixels: &[[f32; 4]],
) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(T::NAME),
        size: wgpu::Extent3d {
            width: pixels.len() as u32,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &half_pixel_bytes(pixels),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(pixels.len() as u32 * 8),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: pixels.len() as u32,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    table.allocate_with_texture(TypeId::of::<T>(), T::NAME, texture, view);
}

#[test]
fn production_composite_shader_reads_back_t4_gpu_sentinels() {
    const WIDTH: u32 = 4;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("need adapter for the production composite readback");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("need device for the production composite readback");
    let scene = [[0.2, 0.4, 0.8, 1.0]; WIDTH as usize];
    let transparent = [
        [0.5, 0.0, 0.0, 0.5],
        [0.0, 0.25, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        [0.5, 0.0, 0.0, 0.5],
    ];
    let water = [
        [0.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        [0.1, 0.3, 0.5, 0.25],
    ];
    let clear = [[0.0, 0.0, 0.0, 0.0]; WIDTH as usize];
    let mut resources = ResourceTable::new();
    allocate_input::<SceneColor>(&device, &queue, &mut resources, &scene);
    allocate_input::<ReflectionTexture>(&device, &queue, &mut resources, &clear);
    allocate_input::<WaterColor>(&device, &queue, &mut resources, &water);
    allocate_input::<CloudColor>(&device, &queue, &mut resources, &clear);
    allocate_input::<GodRayColor>(&device, &queue, &mut resources, &clear);
    allocate_input::<GPosition>(&device, &queue, &mut resources, &clear);
    allocate_input::<GNormal>(&device, &queue, &mut resources, &clear);
    allocate_input::<GAlbedo>(&device, &queue, &mut resources, &clear);
    allocate_input::<GMaterial>(&device, &queue, &mut resources, &clear);
    allocate_input::<TransparentColor>(&device, &queue, &mut resources, &transparent);

    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("T4 Composite Sentinel Output"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    resources.allocate_with_texture(
        TypeId::of::<PostProcessInput>(),
        PostProcessInput::NAME,
        output,
        output_view,
    );

    let mut composite =
        CompositePass::new_with_transparency(&device, wgpu::TextureFormat::Bgra8UnormSrgb);
    composite.resolve(&device, &resources);
    let target = resources.get(resources.handle::<PostProcessInput>());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("T4 Composite Sentinel"),
    });
    composite.execute(&mut encoder, &resources, target);
    queue.submit(std::iter::once(encoder.finish()));

    let bytes_per_row = 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("T4 Composite Sentinel Readback"),
        size: bytes_per_row as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("T4 Composite Sentinel Copy"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: resources
                .texture(resources.handle::<PostProcessInput>())
                .expect("composite output is owned by the resource table"),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(1),
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(std::iter::once(encoder.finish()));
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receiver.recv().unwrap().unwrap();
    let mapped = readback.slice(..).get_mapped_range();
    let actual: Vec<[u8; 3]> = (0..WIDTH)
        .map(|pixel| {
            let offset = (pixel * 8) as usize;
            std::array::from_fn(|channel| {
                let value = half::f16::from_le_bytes([
                    mapped[offset + channel * 2],
                    mapped[offset + channel * 2 + 1],
                ])
                .to_f32();
                (value * 255.0).round() as u8
            })
        })
        .collect();
    assert_eq!(
        actual,
        vec![
            [153, 51, 102],
            [51, 166, 204],
            [51, 102, 204],
            [121, 57, 108]
        ],
        "production composite output must match the four frozen RGBA8 sentinels"
    );
}
