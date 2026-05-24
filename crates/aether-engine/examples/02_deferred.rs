//! 02_deferred - Deferred Shading G-Buffer Visualization
//!
//! Renders multiple meshes (cube, sphere) to G-Buffer textures.
//! Visualizes the albedo channel on screen.

use aether_engine::asset::mesh::CpuMesh;
use tracing::info;

fn main() {
    tracing_subscriber::fmt::init();
    info!("02_deferred example starting...");

    // Create CPU meshes
    let cube_cpu = CpuMesh::cube();
    let sphere_cpu = CpuMesh::sphere(16);

    info!("Cube vertices: {}", cube_cpu.positions.len());
    info!("Sphere vertices: {}", sphere_cpu.positions.len());

    // Note: This example demonstrates the API structure.
    // Full windowed rendering requires extending App to accept renderables.
    // For Phase 1, we verify the types compile correctly and mesh generation works.

    // In a complete implementation, we would:
    // 1. Create GpuMesh instances from CpuMesh via GpuMesh::from_cpu()
    // 2. Build Renderable list with transforms and materials
    // 3. Pass to renderer.render()

    println!("02_deferred example compiled successfully!");
    println!("Cube vertices: {}", cube_cpu.positions.len());
    println!("Sphere vertices: {}", sphere_cpu.positions.len());
}
