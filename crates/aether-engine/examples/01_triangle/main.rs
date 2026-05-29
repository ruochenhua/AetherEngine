//! Aether Engine Bootstrap Example
//!
//! Minimal runnable program demonstrating:
//! - winit window creation
//! - wgpu initialization
//! - Colored triangle rendering
//! - egui debug overlay
//!
//! This thin wrapper exists so that `cargo run --example 01_triangle` still
//! works.  The actual logic lives in `aether_engine::examples::TriangleExample`.

fn main() {
    tracing_subscriber::fmt::init();
    aether_engine::examples::run_standalone(
        aether_engine::examples::TriangleExample::new(),
        "Aether Engine - Bootstrap",
    );
}
