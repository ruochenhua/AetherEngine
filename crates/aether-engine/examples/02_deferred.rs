//! 02_deferred - Deferred Shading with Lighting
//!
//! Renders a cube and sphere with Blinn-Phong lighting.
//! Demonstrates the full deferred pipeline: G-Buffer -> Lighting -> Screen.

use aether_engine::{
    asset::mesh::{CpuMesh, GpuMesh},
    renderer::context::RenderContext,
    renderer::passes::gbuffer::{MaterialUniform, Renderable},
    renderer::passes::lighting::{DirectionalLight, LightingUniforms},
    renderer::Renderer,
};
use glam::{Mat4, Vec3};
use std::sync::Arc;
use tracing::{error, info};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

fn main() {
    tracing_subscriber::fmt::init();
    info!("02_deferred example starting...");

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Aether Engine - Deferred Shading")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720))
            .build(&event_loop)
            .expect("Failed to create window"),
    );

    let mut render_context = pollster::block_on(RenderContext::new(&window));
    let mut renderer = Renderer::new(&render_context, 1280, 720);

    // Create GPU meshes
    let cube_cpu = CpuMesh::cube();
    let sphere_cpu = CpuMesh::sphere(16);
    let cube_gpu = Arc::new(GpuMesh::from_cpu(&render_context.device, &cube_cpu,
    ));
    let sphere_gpu = Arc::new(GpuMesh::from_cpu(
        &render_context.device,
        &sphere_cpu,
    ));

    // Build renderables: cube + sphere
    let renderables = vec![
        Renderable {
            mesh: cube_gpu.clone(),
            transform: Mat4::from_translation(Vec3::new(-0.8, 0.0, 0.0)),
            material: MaterialUniform {
                albedo: [0.8, 0.3, 0.2, 1.0],
                roughness: 0.5,
                metallic: 0.0,
                _pad: [0.0, 0.0],
            },
        },
        Renderable {
            mesh: sphere_gpu.clone(),
            transform: Mat4::from_translation(Vec3::new(0.8, 0.0, 0.0)),
            material: MaterialUniform {
                albedo: [0.2, 0.5, 0.8, 1.0],
                roughness: 0.3,
                metallic: 0.1,
                _pad: [0.0, 0.0],
            },
        },
    ];

    // Hard-coded camera
    let camera_pos = Vec3::new(3.0, 3.0, 3.0);
    let view = Mat4::look_at_rh(camera_pos, Vec3::ZERO, Vec3::Y);
    let proj = Mat4::perspective_rh(
        45.0f32.to_radians(),
        1280.0 / 720.0,
        0.1,
        100.0,
    );

    // Lighting uniforms
    let lighting_uniforms = LightingUniforms {
        camera_pos: camera_pos.into(),
        _pad1: 0.0,
        light: DirectionalLight {
            direction: [-1.0, -1.0, -1.0],
            _pad: 0.0,
            color: [1.0, 0.95, 0.9],
            intensity: 1.2,
        },
        ambient_intensity: 0.15,
        _pad2: [0.0; 3],
    };

    event_loop.set_control_flow(ControlFlow::Poll);

    event_loop
        .run(move |event, elwt| {
            match event {
                Event::WindowEvent { event, .. } => {
                    match event {
                        WindowEvent::CloseRequested => {
                            info!("Window close requested");
                            elwt.exit();
                        }
                        WindowEvent::Resized(physical_size) => {
                            renderer.resize(
                                &mut render_context,
                                physical_size.width,
                                physical_size.height,
                            );
                        }
                        WindowEvent::RedrawRequested => {
                            match renderer.render(
                                &render_context,
                                &renderables,
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
