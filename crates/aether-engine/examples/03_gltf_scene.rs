//! 03_gltf_scene – Scene with Orbit Camera + Ground Plane
//!
//! Renders a multi-object scene with deferred shading and an orbit camera.
//! (GLTF/OBJ loading is planned but not yet implemented.)

fn main() {
    tracing_subscriber::fmt::init();
    aether_engine::examples::run_standalone(
        aether_engine::examples::GltfSceneExample::with_default_model(),
        "Aether Engine - Scene Loading",
    );
}
