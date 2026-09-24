use super::*;
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
}

#[test]
fn init_creates_resources() {
    let _pass = CompositePass::new(&headless_device(), wgpu::TextureFormat::Bgra8UnormSrgb);
}
