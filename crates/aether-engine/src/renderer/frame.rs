//! Per-frame data context passed to all passes before execution.
//!
//! `RenderFrame` is a read-only reference bag injected by the Launcher/Scheduler
//! into each pass via `Pass::apply_frame()`. Passes extract only the fields
//! they need — no pass is forced to depend on data it doesn't consume.
//!
//! This is deliberately a fixed set of references, not a per-pass mutable struct.
//! Adding a pass that needs new data means adding a field here, but the number
//! of distinct data categories in a deferred renderer is bounded (~4-6 fields).

use crate::ecs::World;
use crate::renderer::{camera::FlyCamera, extract::RenderBatch, light::LightingUniforms};
use std::sync::Arc;

/// Read-only per-frame data available to every pass.
///
/// Most fields are references — the Scheduler/Launcher owns the data.
/// `batches` is an `Arc` so the hottest passes can share the extracted batch
/// list without cloning it every frame.
/// Passes that don't need a particular field simply ignore it.
pub struct RenderFrame<'a> {
    /// Render batches extracted from the ECS World; shared by GBufferPass, ShadowPass.
    pub batches: Arc<[RenderBatch]>,
    /// Camera state (position, view/proj matrices).
    pub camera: &'a FlyCamera,
    /// Lighting uniforms (directional light, ambient, debug mode).
    pub lighting: &'a LightingUniforms,
    /// wgpu queue for uniform buffer uploads within pass execution.
    pub queue: &'a wgpu::Queue,
    /// Current aspect ratio (width / height). Used for projection matrices.
    pub aspect: f32,
    /// Frame delta time in seconds.
    pub delta_time: f32,
    /// ECS world reference for passes that read scene components (e.g. TerrainPass).
    pub world: &'a World,
}
