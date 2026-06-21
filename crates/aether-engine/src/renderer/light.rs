// ---------------------------------------------------------------------------
// GPU uniform types (shared between scene loader and lighting pass)
// ---------------------------------------------------------------------------

/// Directional light uniform data.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DirectionalLight {
    /// Light direction (pointing FROM the light).
    pub direction: [f32; 3],
    /// Padding.
    pub _pad: f32,
    /// Light color.
    pub color: [f32; 3],
    /// Light intensity.
    pub intensity: f32,
}

impl Default for DirectionalLight {
    fn default() -> Self {
        Self {
            direction: [-1.0, -1.0, -1.0],
            _pad: 0.0,
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
        }
    }
}

/// Lighting uniform data sent to the lighting shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightingUniforms {
    /// Camera world position.
    pub camera_pos: [f32; 3],
    /// Padding.
    pub _pad1: f32,
    /// Directional light.
    pub light: DirectionalLight,
    /// Ambient light intensity.
    pub ambient_intensity: f32,
    /// Debug visualization mode:
    /// 0 = full lighting, 1 = ambient only, 2 = diffuse only,
    /// 3 = specular only, 4 = normals, 5 = NdotL, 6 = shadow depth.
    pub debug_mode: u32,
    /// Slope-scale depth bias for shadow sampling (NDC units, base value).
    /// Formula: bias = base * tan(acos(NdotL)), clamped to base*10.
    pub shadow_normal_bias: f32,
    /// Shadow map texture size in pixels (width == height).
    pub shadow_map_size: f32,
    /// Light-space view-projection matrices for each cascade.
    pub cascade_view_projs: [[[f32; 4]; 4]; 4],
    /// Far split distances for each cascade.
    pub cascade_splits: [f32; 4],
    /// Number of active cascades.
    pub cascade_count: u32,
    /// Inverse view-projection matrix for skybox view-ray reconstruction.
    pub inv_view_proj: [[f32; 4]; 4],
    /// Feature toggle: SSAO enabled (0 = off, 1 = on).
    pub ssao_enabled: u32,
    /// Feature toggle: shadow mapping enabled (0 = off, 1 = on).
    pub shadow_enabled: u32,
    /// Feature toggle: IBL enabled (0 = off, 1 = on).
    pub ibl_enabled: u32,
    /// Padding to 16-byte alignment.
    pub _pad4: u32,
}

impl Default for LightingUniforms {
    fn default() -> Self {
        Self {
            camera_pos: [3.0, 3.0, 3.0],
            _pad1: 0.0,
            light: DirectionalLight::default(),
            ambient_intensity: 0.1,
            debug_mode: 0,
            shadow_normal_bias: 0.001,
            shadow_map_size: 2048.0,
            cascade_view_projs: [[[0.0; 4]; 4]; 4],
            cascade_splits: [0.0; 4],
            cascade_count: 4,
            inv_view_proj: [[0.0; 4]; 4],
            ssao_enabled: 1,
            shadow_enabled: 1,
            ibl_enabled: 1,
            _pad4: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Light types
// ---------------------------------------------------------------------------

/// Light type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LightType {
    /// Directional light (sun/moon).
    Directional,
    /// Point light (omnidirectional).
    Point,
    /// Spot light (cone).
    Spot,
}
