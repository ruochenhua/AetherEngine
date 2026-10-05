pub(super) async fn request_device(
    adapter: &wgpu::Adapter,
) -> Result<(wgpu::Device, wgpu::Queue), wgpu::RequestDeviceError> {
    let required_limits =
        crate::renderer::passes::lighting::local_lights::required_device_limits(&adapter.limits());
    adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("Device"),
            required_features: wgpu::Features::empty(),
            required_limits,
            experimental_features: Default::default(),
            memory_hints: Default::default(),
            trace: Default::default(),
        })
        .await
}
