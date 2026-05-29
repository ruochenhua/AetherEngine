//! 03_gltf_scene – Scene Loading with GLTF/OBJ + Deferred Shading
//!
//! Loads a 3D model from GLTF or OBJ and renders it with the deferred pipeline.
//! Usage: cargo run --example 03_gltf_scene [-- path/to/model.glb]

fn main() {
    let model_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "crates/aether-engine/examples/assets/duck.glb".to_string());

    tracing_subscriber::fmt::init();
    aether_engine::examples::run_standalone(
        aether_engine::examples::GltfSceneExample::new(model_path),
        "Aether Engine - Scene Loading",
    );
}
