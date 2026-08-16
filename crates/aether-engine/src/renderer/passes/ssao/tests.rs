
use super::*;
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
fn signature_declares_reads_and_writes() {
    let device = headless_device();
    let sig = SSAOPass::new(&device).signature();
    assert_eq!(sig.name, "SSAO");
    assert_eq!(sig.reads.len(), 2);
    assert_eq!(sig.writes.len(), 1);
    assert!(sig.writes[0].type_id == TypeId::of::<AOTexture>() && sig.writes[0].name == "ao");
}

#[test]
fn init_creates_resources() {
    let _pass = SSAOPass::new(&headless_device());
}
