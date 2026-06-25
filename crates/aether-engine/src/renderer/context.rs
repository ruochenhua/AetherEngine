use std::sync::Arc;
use winit::window::Window;

/// Rendering context.
///
/// Holds the wgpu device, queue, surface, and the owned window that the
/// surface is tied to. Keeping the `Arc<Window>` alongside the surface lets
/// wgpu manage the surface lifetime safely without any `unsafe` transmutes.
pub struct RenderContext {
    /// wgpu instance.
    pub instance: wgpu::Instance,
    /// Owned window. The surface holds a reference to it, so the window must
    /// outlive the surface.
    pub window: Arc<Window>,
    /// Surface for presenting.
    pub surface: wgpu::Surface<'static>,
    /// GPU device.
    pub device: wgpu::Device,
    /// Command queue.
    pub queue: wgpu::Queue,
    /// Surface configuration.
    pub config: wgpu::SurfaceConfiguration,
    /// Adapter info.
    pub adapter_info: wgpu::AdapterInfo,
    /// Format used for the 3D render passes (usually sRGB so hardware gamma
    /// correction is applied). The surface itself uses a non-sRGB format to
    /// keep `egui-wgpu` happy; this format is exposed as a surface view format.
    pub render_target_format: wgpu::TextureFormat,
}

impl RenderContext {
    /// Create a new render context for the given window.
    pub async fn new(window: Arc<Window>) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());

        // Pass an owned clone of the Arc<Window> so the surface can keep the
        // window alive. Because `Arc<Window>` is `'static`, the returned
        // `Surface` is also `'static` without an `unsafe` transmute.
        let surface = instance
            .create_surface(Arc::clone(&window))
            .expect("Failed to create surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Failed to find suitable GPU adapter");

        let adapter_info = adapter.get_info();

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: Default::default(),
                memory_hints: Default::default(),
                trace: Default::default(),
            })
            .await
            .expect("Failed to create device");

        let surface_caps = surface.get_capabilities(&adapter);

        // Use a non-sRGB surface format so egui-wgpu can use its preferred
        // gamma-space shader path and avoid the "linear framebuffer" warning.
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| {
                !f.is_srgb()
                    && matches!(
                        f,
                        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                    )
            })
            .copied()
            .unwrap_or_else(|| surface_caps.formats[0]);

        // The 3D pipeline still renders to an sRGB view for correct hardware
        // gamma encoding. We request that format as a surface view format.
        let render_target_format = surface_caps
            .formats
            .iter()
            .find(|f| {
                f.is_srgb()
                    && matches!(
                        f,
                        wgpu::TextureFormat::Bgra8UnormSrgb | wgpu::TextureFormat::Rgba8UnormSrgb
                    )
            })
            .copied()
            .unwrap_or(surface_format);

        let view_formats = if render_target_format != surface_format {
            vec![render_target_format]
        } else {
            vec![]
        };

        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Immediate,
            alpha_mode: *surface_caps
                .alpha_modes
                .first()
                .unwrap_or(&wgpu::CompositeAlphaMode::Opaque),
            view_formats,
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        Self {
            instance,
            window,
            surface,
            device,
            queue,
            config,
            adapter_info,
            render_target_format,
        }
    }

    /// Resize the surface.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    /// Get the surface format (non-sRGB, used for presenting and egui).
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// Get the format used for the 3D render target view (usually sRGB).
    pub fn render_target_format(&self) -> wgpu::TextureFormat {
        self.render_target_format
    }

    /// Get the current surface texture.
    pub fn get_current_texture(&self) -> wgpu::CurrentSurfaceTexture {
        self.surface.get_current_texture()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// `RenderContext::new` must accept an owned `Arc<Window>` instead of a
    /// borrowed reference, so the context can hold the window and surface with
    /// matching lifetimes without unsafe transmutes.
    #[test]
    fn constructor_accepts_owned_window() {
        fn _type_check(window: Arc<Window>) {
            let _ctx = pollster::block_on(RenderContext::new(window));
        }
        // Full window+surface creation is platform-specific and exercised by
        // the launcher; this test ensures the public signature is correct.
        assert!(std::mem::size_of::<RenderContext>() != 0);
    }
}
