//! Per-frame data context passed to all passes before execution.
//!
//! `RenderFrame` is a read-only reference bag injected by the Launcher/Scheduler
//! into each pass via `Pass::apply_frame()`. Passes extract only the fields
//! they need — no pass is forced to depend on data it doesn't consume.
//!
//! This is deliberately a fixed set of references, not a per-pass mutable struct.
//! Adding a pass that needs new data means adding a field here, but the number
//! of distinct data categories in a deferred renderer is bounded (~4-6 fields).

use crate::renderer::{camera::FlyCamera, light::LightingUniforms, renderable::Renderable};

/// Read-only per-frame data available to every pass.
///
/// All fields are references — the Scheduler/Launcher owns the data.
/// Passes that don't need a particular field simply ignore it.
pub struct RenderFrame<'a> {
    /// Renderable objects (mesh + transform + material); shared by GBufferPass, ShadowPass.
    pub renderables: &'a [Renderable],
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
}
