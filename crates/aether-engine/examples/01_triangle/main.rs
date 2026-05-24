//! Aether Engine Bootstrap Example
//!
//! Minimal runnable program demonstrating:
//! - winit window creation
//! - wgpu initialization
//! - Colored triangle rendering
//! - egui debug overlay

use std::sync::Arc;
use tracing::{error, info};
use wgpu::util::DeviceExt;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

// ---------------------------------------------------------------------------
// Vertex data
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

impl Vertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

const VERTICES: &[Vertex] = &[
    Vertex {
        position: [0.0, 0.5, 0.0],
        color: [1.0, 0.0, 0.0],
    },
    Vertex {
        position: [-0.5, -0.5, 0.0],
        color: [0.0, 1.0, 0.0],
    },
    Vertex {
        position: [0.5, -0.5, 0.0],
        color: [0.0, 0.0, 1.0],
    },
];

// ---------------------------------------------------------------------------
// Shaders
// ---------------------------------------------------------------------------

const SHADER_WGSL: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) color: vec3<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(position, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
"#;

// ---------------------------------------------------------------------------
// Render context
// ---------------------------------------------------------------------------

#[allow(dead_code)]
struct RenderContext {
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    adapter_info: wgpu::AdapterInfo,
}

impl RenderContext {
    async fn new(window: Arc<winit::window::Window>) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .expect("Failed to create surface");
        // SAFETY: Window lives for the entire application lifetime.
        let surface =
            unsafe { std::mem::transmute::<wgpu::Surface<'_>, wgpu::Surface<'static>>(surface) };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Failed to find suitable GPU adapter");

        let adapter_info = adapter.get_info();
        info!("GPU Adapter: {} ({:?})", adapter_info.name, adapter_info.backend);

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
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
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

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    tracing_subscriber::fmt::init();
    info!("Aether Engine Bootstrap starting...");

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Aether Engine - Bootstrap")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720))
            .build(&event_loop)
            .expect("Failed to create window"),
    );

    let mut ctx = pollster::block_on(RenderContext::new(window.clone()));

    // --- Triangle pipeline ---
    let shader = ctx
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Triangle Shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_WGSL.into()),
        });

    let pipeline_layout = ctx
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

    let render_pipeline = ctx
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: ctx.config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

    let vertex_buffer = ctx.device.create_buffer_init(
        &wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        },
    );

    // --- egui setup ---
    let egui_ctx = egui::Context::default();
    let viewport_id = egui_ctx.viewport_id();
    let mut egui_winit_state = egui_winit::State::new(
        egui_ctx.clone(),
        viewport_id,
        &window,
        None,
        None,
    );
    let mut egui_renderer = egui_wgpu::Renderer::new(
        &ctx.device,
        ctx.config.format,
        None,
        1,
    );

    // --- Frame timing ---
    let mut last_frame_time = std::time::Instant::now();
    let mut fps: f32 = 0.0;

    event_loop.set_control_flow(ControlFlow::Poll);

    event_loop
        .run(move |event, elwt| {
            match event {
                Event::WindowEvent { event, .. } => {
                    let egui_response = egui_winit_state.on_window_event(&window, &event);
                    if egui_response.consumed {
                        return;
                    }

                    match event {
                        WindowEvent::CloseRequested => {
                            info!("Window close requested");
                            elwt.exit();
                        }
                        WindowEvent::Resized(physical_size) => {
                            ctx.resize(physical_size.width, physical_size.height);
                        }
                        WindowEvent::RedrawRequested => {
                            // --- Timing ---
                            let now = std::time::Instant::now();
                            let dt = now.duration_since(last_frame_time).as_secs_f32();
                            last_frame_time = now;
                            let frame_time_ms = dt * 1000.0;
                            if dt > 0.0 {
                                fps = fps * 0.9 + (1.0 / dt) * 0.1;
                            }

                            // --- Prepare egui ---
                            let raw_input = egui_winit_state.take_egui_input(&window);
                            let egui_output = egui_ctx.run(raw_input, |ctx| {
                                egui::Window::new("Aether Debug")
                                    .resizable(false)
                                    .collapsible(false)
                                    .anchor(egui::Align2::RIGHT_BOTTOM, [-8.0, -8.0])
                                    .show(ctx, |ui| {
                                        ui.label(format!("FPS: {:.1}", fps));
                                        ui.label(format!("Frame: {:.2} ms", frame_time_ms));
                                        ui.label(format!(
                                            "Resolution: {}x{}",
                                            ctx.screen_rect().width() as u32,
                                            ctx.screen_rect().height() as u32
                                        ));
                                    });
                            });
                            egui_winit_state.handle_platform_output(&window, egui_output.platform_output);

                            let paint_jobs = egui_ctx.tessellate(egui_output.shapes, egui_output.pixels_per_point);
                            let screen_descriptor = egui_wgpu::ScreenDescriptor {
                                size_in_pixels: [ctx.config.width, ctx.config.height],
                                pixels_per_point: window.scale_factor() as f32,
                            };

                            // --- Render ---
                            let output = match ctx.surface.get_current_texture() {
                                Ok(o) => o,
                                Err(wgpu::SurfaceError::Lost) => {
                                    ctx.resize(ctx.config.width, ctx.config.height);
                                    return;
                                }
                                Err(wgpu::SurfaceError::OutOfMemory) => {
                                    error!("GPU out of memory");
                                    elwt.exit();
                                    return;
                                }
                                Err(e) => {
                                    error!("Surface error: {:?}", e);
                                    return;
                                }
                            };
                            let view = output
                                .texture
                                .create_view(&wgpu::TextureViewDescriptor::default());

                            let mut encoder = ctx
                                .device
                                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                    label: Some("Render Encoder"),
                                });

                            // Triangle pass
                            {
                                let mut render_pass = encoder.begin_render_pass(
                                    &wgpu::RenderPassDescriptor {
                                        label: Some("Triangle Pass"),
                                        color_attachments: &[Some(
                                            wgpu::RenderPassColorAttachment {
                                                view: &view,
                                                resolve_target: None,
                                                ops: wgpu::Operations {
                                                    load: wgpu::LoadOp::Clear(wgpu::Color {
                                                        r: 0.1,
                                                        g: 0.1,
                                                        b: 0.15,
                                                        a: 1.0,
                                                    }),
                                                    store: wgpu::StoreOp::Store,
                                                },
                                            },
                                        )],
                                        depth_stencil_attachment: None,
                                        timestamp_writes: None,
                                        occlusion_query_set: None,
                                    },
                                );
                                render_pass.set_pipeline(&render_pipeline);
                                render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                                render_pass.draw(0..3, 0..1);
                            }

                            // egui pass
                            for (id, image_delta) in &egui_output.textures_delta.set {
                                egui_renderer.update_texture(&ctx.device, &ctx.queue, *id, image_delta);
                            }
                            egui_renderer.update_buffers(
                                &ctx.device,
                                &ctx.queue,
                                &mut encoder,
                                &paint_jobs,
                                &screen_descriptor,
                            );
                            {
                                let mut render_pass = encoder.begin_render_pass(
                                    &wgpu::RenderPassDescriptor {
                                        label: Some("egui Pass"),
                                        color_attachments: &[Some(
                                            wgpu::RenderPassColorAttachment {
                                                view: &view,
                                                resolve_target: None,
                                                ops: wgpu::Operations {
                                                    load: wgpu::LoadOp::Load,
                                                    store: wgpu::StoreOp::Store,
                                                },
                                            },
                                        )],
                                        depth_stencil_attachment: None,
                                        timestamp_writes: None,
                                        occlusion_query_set: None,
                                    },
                                );
                                egui_renderer.render(&mut render_pass, &paint_jobs, &screen_descriptor);
                            }

                            ctx.queue.submit(std::iter::once(encoder.finish()));
                            output.present();
                        }
                        _ => {}
                    }
                }
                Event::AboutToWait => {
                    window.request_redraw();
                }
                _ => {}
            }
        })
        .expect("Event loop error");
}
