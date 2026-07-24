//! Shared utilities for `#[cfg(test)]` code.

/// Attempts to create a headless wgpu device and queue for GPU-dependent tests.
///
/// Returns `None` when no GPU adapter is available (e.g. CI runners without a
/// GPU). Callers must skip loudly instead of silently passing:
///
/// ```ignore
/// let Some((device, queue)) = headless_device_queue() else {
///     eprintln!("SKIP: no GPU adapter available");
///     return;
/// };
/// ```
pub(crate) fn headless_device_queue() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .ok()?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
}
