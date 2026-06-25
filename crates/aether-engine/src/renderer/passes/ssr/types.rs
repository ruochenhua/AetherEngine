//! SSR types.
//!
//! CPU-side mirrors of the data sent to the SSR WGSL shaders.

use bytemuck::{Pod, Zeroable};

/// SSR settings (matches WGSL std140 layout).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct SSRSettings {
    pub(super) camera_pos: [f32; 3],
    pub(super) _pad0: f32,
    pub(super) view_proj: [[f32; 4]; 4],
    pub(super) screen_size: [f32; 2],
    pub(super) _pad1: [f32; 2],
    pub(super) max_distance: f32,
    pub(super) linear_steps: f32,
    pub(super) thickness: f32,
    pub(super) step_exponent: f32,
    pub(super) jitter_amount: f32,
    pub(super) min_roughness: f32,
    pub(super) max_roughness: f32,
    pub(super) edge_fade_start: f32,
    pub(super) edge_fade_end: f32,
    pub(super) ssr_debug_mode: u32,
    pub(super) ssr_enabled: u32,
    pub(super) frame_index: u32,
    pub(super) _pad2: u32,
    pub(super) _pad3: u32,
    pub(super) _pad4: u32,
    pub(super) _pad5: u32,
}
