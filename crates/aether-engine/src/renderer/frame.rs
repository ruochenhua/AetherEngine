//! Per-frame data context passed to all passes before execution.
//!
//! `RenderFrame` is a read-only reference bag injected by the Launcher/Scheduler
//! into each pass via `Pass::apply_frame()`. Passes extract only the fields
//! they need — no pass is forced to depend on data it doesn't consume.
//!
//! This is deliberately a fixed set of references, not a per-pass mutable struct.
//! Adding a pass that needs new data means adding a field here, but the number
//! of distinct data categories in a deferred renderer is bounded (~4-6 fields).

use crate::renderer::{
    camera::FlyCamera,
    extract::{OptionalPassData, RenderBatch},
    light::LightingUniforms,
    passes::{debug::DebugVertex, fxaa::FxaaQuality, tone_mapping::ToneMappingMode},
};
use std::sync::Arc;

/// UI-controllable per-frame parameters.
///
/// All pass-specific configuration that used to be pushed through Scheduler
/// setters now flows through this single data channel. Adding a new controllable
/// parameter only requires adding a field here and reading it in the pass's
/// `apply_frame` implementation.
#[derive(Clone, Debug)]
pub struct FrameConfig {
    /// SSAO on/off.
    pub ssao_enabled: bool,
    /// Shadow mapping on/off.
    pub shadow_enabled: bool,
    /// Image-based lighting on/off.
    pub ibl_enabled: bool,
    /// SSAO sample radius.
    pub ssao_radius: f32,
    /// SSAO depth bias.
    pub ssao_bias: f32,
    /// SSAO intensity multiplier.
    pub ssao_intensity: f32,
    /// Lighting debug visualization mode.
    pub debug_mode: u32,
    /// SSR debug visualization mode.
    pub ssr_debug_mode: u32,
    /// SSR on/off.
    pub ssr_enabled: bool,
    /// SSR temporal jitter frame index.
    pub ssr_frame_index: u32,
    /// Tone mapping operator.
    pub tone_mapping_mode: ToneMappingMode,
    /// Bloom on/off.
    pub bloom_enabled: bool,
    /// Bloom threshold.
    pub bloom_threshold: f32,
    /// Bloom internal intensity.
    pub bloom_intensity: f32,
    /// Bloom compositing intensity.
    pub bloom_composite_intensity: f32,
    /// FXAA on/off.
    pub fxaa_enabled: bool,
    /// FXAA quality preset.
    pub fxaa_quality: FxaaQuality,
    /// FXAA edge threshold (None = use default).
    pub fxaa_edge_threshold: Option<f32>,
    /// Current backbuffer width.
    pub screen_width: u32,
    /// Current backbuffer height.
    pub screen_height: u32,
    /// Dynamic debug lines to draw this frame.
    pub dynamic_lines: Vec<DebugVertex>,
}

impl Default for FrameConfig {
    fn default() -> Self {
        Self {
            ssao_enabled: true,
            shadow_enabled: true,
            ibl_enabled: true,
            ssao_radius: 0.5,
            ssao_bias: 0.025,
            ssao_intensity: 1.5,
            debug_mode: 0,
            ssr_debug_mode: 0,
            ssr_enabled: false,
            ssr_frame_index: 0,
            tone_mapping_mode: ToneMappingMode::ACES,
            bloom_enabled: true,
            bloom_threshold: 1.0,
            bloom_intensity: 1.0,
            bloom_composite_intensity: 0.5,
            fxaa_enabled: true,
            fxaa_quality: FxaaQuality::High,
            fxaa_edge_threshold: None,
            screen_width: 1280,
            screen_height: 720,
            dynamic_lines: Vec::new(),
        }
    }
}

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
    /// Optional scene components extracted from the ECS World for conditional passes
    /// (terrain, water, atmosphere, clouds, god rays).
    pub optional: &'a OptionalPassData,
    /// Per-frame configuration channel.
    pub config: &'a FrameConfig,
}
