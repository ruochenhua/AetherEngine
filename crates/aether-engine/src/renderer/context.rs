use tracing::info;
use winit::window::Window;

/// G-Buffer texture collection.
///
/// Contains all render targets produced by the G-Buffer pass.
/// These are consumed by subsequent passes (lighting, SSAO, etc.).
pub struct GBuffer {
    /// World position (xyz) + 1/w (RGBA16Float).
    pub position: wgpu::TextureView,
    /// World normal (xyz) + padding (RGBA16Float).
    pub normal: wgpu::TextureView,
    /// Base color (RGBA8Unorm).
    pub albedo: wgpu::TextureView,
    /// Roughness + Metallic (RG8Unorm).
    pub material: wgpu::TextureView,
    /// Depth buffer (Depth32Float).
    pub depth: wgpu::TextureView,
    /// Texture width.
    pub width: u32,
    /// Texture height.
    pub height: u32,
}

impl GBuffer {
    /// Create G-Buffer textures with the given size.
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let create_color_texture = |format: wgpu::TextureFormat, label: &str| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            texture.create_view(&wgpu::TextureViewDescriptor::default())
        };

        let create_depth_texture = |label: &str| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            texture.create_view(&wgpu::TextureViewDescriptor::default())
        };

        Self {
            position: create_color_texture(wgpu::TextureFormat::Rgba16Float, "GBuffer_Position"),
            normal: create_color_texture(wgpu::TextureFormat::Rgba16Float, "GBuffer_Normal"),
            albedo: create_color_texture(wgpu::TextureFormat::Rgba8Unorm, "GBuffer_Albedo"),
            material: create_color_texture(wgpu::TextureFormat::Rg8Unorm, "GBuffer_Material"),
            depth: create_depth_texture("GBuffer_Depth"),
            width,
            height,
        }
    }
}

/// Rendering context.
///
/// Holds the wgpu device, queue, and surface.
/// This is the low-level graphics API abstraction.
pub struct RenderContext {
    /// wgpu instance.
    pub instance: wgpu::Instance,
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
}

impl RenderContext {
    /// Create a new render context for the given window.
    pub async fn new(window: &Window) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(window)
            .expect("Failed to create surface");
        // SAFETY: Window lives for the entire application lifetime,
        // so extending Surface lifetime to 'static is sound.
        let surface = unsafe { std::mem::transmute::<wgpu::Surface<'_>, wgpu::Surface<'static>>(surface) };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Failed to find suitable GPU adapter");

        let adapter_info = adapter.get_info();
        info!(
            "GPU Adapter: {} ({:?})",
            adapter_info.name, adapter_info.backend
        );

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .expect("Failed to create device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        Self {
            instance,
            surface,
            device,
            queue,
            config,
            adapter_info,
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

    /// Get the surface format.
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// Get the current surface texture.
    pub fn get_current_texture(&self) -> Result<wgpu::SurfaceTexture, wgpu::SurfaceError> {
        self.surface.get_current_texture()
    }
}
