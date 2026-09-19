use super::*;

fn headless_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("need adapter");
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .expect("need device")
}

fn init_ctx<'a>(device: &'a wgpu::Device, queue: &'a wgpu::Queue) -> InitContext<'a> {
    let texture_cache = Box::leak(Box::new(crate::asset::texture_cache::GpuTextureCache::new(
        device, queue,
    )));
    InitContext {
        device,
        queue,
        surface_format: wgpu::TextureFormat::Bgra8UnormSrgb,
        depth_format: wgpu::TextureFormat::Depth32Float,
        width: 64,
        height: 64,
        ibl_resources: None,
        texture_cache,
    }
}

#[test]
fn signature_ok() {
    let (device, queue) = headless_device();
    let ctx = init_ctx(&device, &queue);
    let pass = GBufferPass::init(&ctx);
    let sig = pass.signature();
    assert_eq!(sig.writes.len(), 5);
}

#[test]
fn resolve_ok() {
    let (device, queue) = headless_device();
    let ctx = init_ctx(&device, &queue);
    let mut pass = GBufferPass::init(&ctx);
    let mut table = ResourceTable::new();
    for (type_id, name, fmt) in [
        (
            std::any::TypeId::of::<GPosition>(),
            GPosition::NAME,
            wgpu::TextureFormat::Rgba16Float,
        ),
        (
            std::any::TypeId::of::<GNormal>(),
            GNormal::NAME,
            wgpu::TextureFormat::Rgba16Float,
        ),
        (
            std::any::TypeId::of::<GAlbedo>(),
            GAlbedo::NAME,
            wgpu::TextureFormat::Rgba8Unorm,
        ),
        (
            std::any::TypeId::of::<GMaterial>(),
            GMaterial::NAME,
            wgpu::TextureFormat::Rg8Unorm,
        ),
        (
            std::any::TypeId::of::<GDepth>(),
            GDepth::NAME,
            wgpu::TextureFormat::Depth32Float,
        ),
    ] {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(name),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: fmt,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        table.allocate(
            type_id,
            name,
            tex.create_view(&wgpu::TextureViewDescriptor::default()),
        );
    }
    pass.resolve(&device, &table);
    assert!(pass.pos_handle.is_some());
}

#[test]
fn shader_preserves_unlit_marker_flag_in_albedo_alpha() {
    naga::front::wgsl::parse_str(shaders::GBUFFER_SHADER_SRC)
        .expect("G-buffer shader should remain valid WGSL");
    assert!(shaders::GBUFFER_SHADER_SRC.contains("f32(obj.unlit)"));
}
