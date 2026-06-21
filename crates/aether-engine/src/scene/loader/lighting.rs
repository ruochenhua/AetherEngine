//! Lighting uniform construction for `SceneLoader`.

use crate::renderer::light::{DirectionalLight, LightingUniforms};
use crate::scene::SceneDescription;

/// Construct lighting uniforms from scene lights and ambient.
pub(super) fn build_lighting_uniforms(desc: &SceneDescription) -> LightingUniforms {
    let light = desc.lights.first().map_or_else(
        || DirectionalLight {
            direction: [0.0, -1.0, 0.0],
            _pad: 0.0,
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
        },
        |cfg| DirectionalLight {
            direction: cfg.direction,
            _pad: 0.0,
            color: cfg.color,
            intensity: cfg.intensity,
        },
    );

    LightingUniforms {
        camera_pos: desc.camera.position,
        _pad1: 0.0,
        light,
        ambient_intensity: desc.ambient,
        debug_mode: 0,
        shadow_normal_bias: 0.001,
        shadow_map_size: 2048.0,
        cascade_view_projs: [[[0.0; 4]; 4]; 4],
        cascade_splits: [0.0; 4],
        cascade_count: 4,
        _pad_cascade: [0; 3],
        inv_view_proj: [[0.0; 4]; 4],
        ssao_enabled: 1,
        shadow_enabled: 1,
        ibl_enabled: 1,
        _pad4: 0,
    }
}
