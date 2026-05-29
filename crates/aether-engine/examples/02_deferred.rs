//! 02_deferred - Deferred Shading with Lighting + Debug Overlay
//!
//! Renders a cube and sphere with Blinn-Phong lighting.
//! Includes egui debug overlay (FPS, frame time, resolution).
//!
//! This thin wrapper exists so that `cargo run --example 02_deferred` still
//! works.  The actual logic lives in `aether_engine::examples::DeferredExample`.

fn main() {
    tracing_subscriber::fmt::init();
    aether_engine::examples::run_standalone(
        aether_engine::examples::DeferredExample::new(),
        "Aether Engine - Deferred Shading",
    );
}
