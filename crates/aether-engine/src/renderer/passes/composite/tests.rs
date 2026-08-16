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
fn init_creates_resources() {
    let _pass = CompositePass::new(&headless_device(), wgpu::TextureFormat::Bgra8UnormSrgb);
}
