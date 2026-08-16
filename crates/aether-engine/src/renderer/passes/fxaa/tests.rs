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
fn signature_declares_reads() {
    let device = headless_device();
    let pass = FXAAPass::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb);
    let sig = pass.signature();
    assert_eq!(sig.name, "FXAA");
    assert_eq!(sig.reads.len(), 1);
    assert_eq!(sig.writes.len(), 1);
}

#[test]
fn init_creates_resources() {
    let _pass = FXAAPass::new(&headless_device(), wgpu::TextureFormat::Bgra8UnormSrgb);
}

#[test]
fn default_quality_is_high() {
    let device = headless_device();
    let pass = FXAAPass::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb);
    assert_eq!(pass.quality, FxaaQuality::High);
}

#[test]
fn quality_can_be_changed() {
    let device = headless_device();
    let mut pass = FXAAPass::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb);
    pass.set_quality(FxaaQuality::Low);
    assert_eq!(pass.quality, FxaaQuality::Low);
    pass.set_quality(FxaaQuality::Medium);
    assert_eq!(pass.quality, FxaaQuality::Medium);
}
