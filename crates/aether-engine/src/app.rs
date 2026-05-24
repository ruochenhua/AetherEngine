use crate::{
    input::InputManager,
    renderer::{context::RenderContext, Renderer},
    renderer::passes::lighting::LightingUniforms,
};
use glam::Mat4;
use tracing::{error, info};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

/// Main application entry point.
/// 
/// Manages the window, event loop, renderer, and scene.
pub struct App {
    title: String,
    width: u32,
    height: u32,
}

impl App {
    /// Create a new application.
    pub fn new(title: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            title: title.into(),
            width,
            height,
        }
    }

    /// Run the application main loop.
    pub fn run(self) {
        info!("Starting Aether Engine: {}", self.title);

        let event_loop = EventLoop::new().expect("Failed to create event loop");
        let window = WindowBuilder::new()
            .with_title(&self.title)
            .with_inner_size(winit::dpi::LogicalSize::new(self.width, self.height))
            .build(&event_loop)
            .expect("Failed to create window");

        let mut render_context = pollster::block_on(RenderContext::new(&window));
        let mut renderer = Renderer::new(&render_context, self.width, self.height);
        let mut input = InputManager::new();

        // Simple camera matrices for Phase 1
        let view = Mat4::look_at_rh(
            glam::Vec3::new(0.0, 0.0, 3.0),
            glam::Vec3::new(0.0, 0.0, 0.0),
            glam::Vec3::Y,
        );
        let proj = Mat4::perspective_rh(
            45.0f32.to_radians(),
            self.width as f32 / self.height as f32,
            0.1,
            100.0,
        );

        event_loop.set_control_flow(ControlFlow::Poll);

        event_loop
            .run(move |event, elwt| {
                match event {
                    Event::WindowEvent { event, .. } => {
                        input.handle_window_event(&event);

                        match event {
                            WindowEvent::CloseRequested => {
                                info!("Window close requested");
                                elwt.exit();
                            }
                            WindowEvent::Resized(physical_size) => {
                                renderer.resize(&mut render_context, physical_size.width, physical_size.height);
                            }
                            WindowEvent::RedrawRequested => {
                                // Update phase
                                renderer.update(1.0 / 60.0);

                                // Lighting uniforms (temporary hard-coded)
                                let lighting_uniforms = LightingUniforms::default();

                                // Render phase
                                match renderer.render(
                                    &render_context,
                                    &[], // No renderables yet (populated by examples)
                                    &view,
                                    &proj,
                                    &lighting_uniforms,
                                ) {
                                    Ok(_) => {}
                                    Err(wgpu::SurfaceError::Lost) => {
                                        renderer.resize(
                                            &mut render_context,
                                            window.inner_size().width,
                                            window.inner_size().height,
                                        );
                                    }
                                    Err(wgpu::SurfaceError::OutOfMemory) => {
                                        error!("GPU out of memory");
                                        elwt.exit();
                                    }
                                    Err(e) => {
                                        error!("Render error: {:?}", e);
                                    }
                                }
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
}
