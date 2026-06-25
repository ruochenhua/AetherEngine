//! SSR Pass — Screen-Space Reflection
//!
//! Full-screen quad pass that traces reflection rays in screen space.
//! Reads G-Buffer and scene color, outputs reflection contribution.
//!
//! Pipeline: LightingPass(→SceneColor) → SSRPass(→ReflectionTexture)
//!           → CompositePass(→swapchain)

use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{InitContext, Pass, PassSignature, ResHandle};
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;

mod execute;
mod pipeline;
mod types;

/// SSR pass state.
pub struct SSRPass {
    trace_pipeline: wgpu::RenderPipeline,
    upsample_pipeline: wgpu::RenderPipeline,
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,
    settings_buffer: wgpu::Buffer,
    settings_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    settings_bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    dummy_texture_view: wgpu::TextureView,
    // Handles populated by resolve
    pos_handle: Option<ResHandle<GPosition>>,
    normal_handle: Option<ResHandle<GNormal>>,
    material_handle: Option<ResHandle<GMaterial>>,
    depth_handle: Option<ResHandle<GDepth>>,
    scene_color_handle: Option<ResHandle<SceneColor>>,
    trace_bind_group: Option<wgpu::BindGroup>,
    upsample_bind_group: Option<wgpu::BindGroup>,
    // Per-frame / mutable
    ssr_debug_mode: u32,
    ssr_enabled: u32,
    frame_index: u32,
    screen_size: [f32; 2],
    trace_width: u32,
    trace_height: u32,
}

impl Pass for SSRPass {
    fn name(&self) -> &str {
        "SSR"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("SSR")
            .read::<GPosition>()
            .read::<GNormal>()
            .read::<GMaterial>()
            .read::<GDepth>()
            .read::<SceneColor>()
            .write_sized::<SsrTraceResult>(
                wgpu::TextureFormat::Rgba16Float,
                self.trace_width.max(1),
                self.trace_height.max(1),
            )
            .write::<ReflectionTexture>(wgpu::TextureFormat::Rgba16Float)
    }

    fn init(ctx: &InitContext) -> Self {
        Self::new(ctx.device)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.pos_handle = Some(resources.handle::<GPosition>());
        self.normal_handle = Some(resources.handle::<GNormal>());
        self.material_handle = Some(resources.handle::<GMaterial>());
        self.depth_handle = Some(resources.handle::<GDepth>());
        self.scene_color_handle = Some(resources.handle::<SceneColor>());

        let pos_view = resources.get(self.pos_handle.unwrap());
        let norm_view = resources.get(self.normal_handle.unwrap());
        let material_view = resources.get(self.material_handle.unwrap());
        let depth_view = resources.get(self.depth_handle.unwrap());
        let scene_color_view = resources.get(self.scene_color_handle.unwrap());
        let trace_view = resources.get(resources.handle::<SsrTraceResult>());

        // Trace bind group: binds a 1x1 dummy at binding 6 (layout requires 7 bindings)
        // to avoid conflict with SsrTraceResult being used as COLOR_TARGET in this pass
        self.trace_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSR Trace Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(pos_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(norm_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(material_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(scene_color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&self.dummy_texture_view),
                },
            ],
        }));
        // Upsample bind group: includes SsrTraceResult for reading
        self.upsample_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSR Upsample Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(pos_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(norm_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(material_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(scene_color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(trace_view),
                },
            ],
        }));
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        self.set_enabled(frame.config.ssr_enabled);
        self.set_screen_size(frame.config.screen_width, frame.config.screen_height);
        self.set_debug_mode(frame.config.ssr_debug_mode);
        self.set_frame_index(frame.config.ssr_frame_index);

        let proj = frame.camera.projection_matrix(frame.aspect);
        let view = frame.camera.view_matrix();
        let view_proj = proj * view;

        let settings = types::SSRSettings {
            camera_pos: frame.camera.position.into(),
            _pad0: 0.0,
            view_proj: view_proj.to_cols_array_2d(),
            screen_size: self.screen_size,
            _pad1: [0.0; 2],
            max_distance: 20.0,
            linear_steps: 12.0,
            thickness: 0.5,
            step_exponent: 1.0,
            jitter_amount: 1.0,
            min_roughness: 0.08,
            max_roughness: 0.6,
            edge_fade_start: 0.0,
            edge_fade_end: 0.25,
            ssr_debug_mode: self.ssr_debug_mode,
            ssr_enabled: self.ssr_enabled,
            frame_index: self.frame_index,
            _pad2: 0,
            _pad3: 0,
            _pad4: 0,
            _pad5: 0,
        };
        frame
            .queue
            .write_buffer(&self.settings_buffer, 0, bytemuck::cast_slice(&[settings]));
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        sv: &wgpu::TextureView,
    ) {
        execute::execute(self, encoder, resources, sv);
    }
}

impl SSRPass {
    /// Create a new SSR pass.
    pub fn new(device: &wgpu::Device) -> Self {
        let objects = pipeline::create_device_objects(device);

        Self {
            trace_pipeline: objects.trace_pipeline,
            upsample_pipeline: objects.upsample_pipeline,
            quad_vertex_buffer: objects.quad_vertex_buffer,
            quad_vertex_count: objects.quad_vertex_count,
            settings_buffer: objects.settings_buffer,
            settings_bind_group: objects.settings_bind_group,
            texture_bind_group_layout: objects.texture_bind_group_layout,
            settings_bind_group_layout: objects.settings_bind_group_layout,
            sampler: objects.sampler,
            dummy_texture_view: objects.dummy_texture_view,
            pos_handle: None,
            normal_handle: None,
            material_handle: None,
            depth_handle: None,
            scene_color_handle: None,
            trace_bind_group: None,
            upsample_bind_group: None,
            ssr_debug_mode: 0,
            ssr_enabled: 1,
            frame_index: 0,
            screen_size: [1280.0, 720.0],
            trace_width: 640,
            trace_height: 360,
        }
    }

    /// Set SSR debug visualization mode.
    pub fn set_debug_mode(&mut self, mode: u32) {
        self.ssr_debug_mode = mode;
    }

    /// Set the frame index for deterministic per-frame jitter.
    pub fn set_frame_index(&mut self, index: u32) {
        self.frame_index = index;
    }

    /// Enable or disable SSR.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.ssr_enabled = if enabled { 1 } else { 0 };
    }

    /// Update screen dimensions.
    pub fn set_screen_size(&mut self, width: u32, height: u32) {
        self.screen_size = [width as f32, height as f32];
        self.trace_width = width / 2;
        self.trace_height = height / 2;
    }
}

#[cfg(test)]
mod tests {
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
    fn signature_declares_correct_resources() {
        let device = headless_device();
        let sig = SSRPass::new(&device).signature();
        assert_eq!(sig.name, "SSR");
        assert_eq!(sig.reads.len(), 5);
        assert_eq!(sig.writes.len(), 2);
        assert!(
            sig.writes[1].type_id == TypeId::of::<ReflectionTexture>()
                && sig.writes[1].name == "reflection"
        );
    }

    #[test]
    fn init_creates_resources() {
        let _pass = SSRPass::new(&headless_device());
    }
}
