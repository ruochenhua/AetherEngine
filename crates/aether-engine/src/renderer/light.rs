use glam::Vec3;

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
    #[allow(dead_code)]
    pub(crate) _pad2: [f32; 2],
    /// Light-space view-projection for shadow sampling.
    pub light_view_proj: [[f32; 4]; 4],
}

impl Default for LightingUniforms {
    fn default() -> Self {
        Self {
            camera_pos: [3.0, 3.0, 3.0],
            _pad1: 0.0,
            light: DirectionalLight::default(),
            ambient_intensity: 0.1,
            debug_mode: 0,
            _pad2: [0.0; 2],
            light_view_proj: [[0.0; 4]; 4],
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

/// Light component.
///
/// Pure data - rendering logic is handled by `ShadowPass` and `LightingPass`.
#[derive(Debug, Clone)]
pub struct Light {
    /// Light type.
    pub light_type: LightType,
    /// Light color (RGB).
    pub color: Vec3,
    /// Light intensity.
    pub intensity: f32,
    /// Whether this light casts shadows.
    pub cast_shadow: bool,
}


impl Default for Light {
    fn default() -> Self {
        Self {
            light_type: LightType::Directional,
            color: Vec3::ONE,
            intensity: 1.0,
            cast_shadow: true,
        }
    }
}

impl Light {
    /// Create a directional light.
    pub fn directional(_direction: Vec3, color: Vec3, intensity: f32) -> Self {
        Self {
            light_type: LightType::Directional,
            color,
            intensity,
            cast_shadow: true,
        }
    }

    /// Create a point light.
    pub fn point(color: Vec3, intensity: f32, _range: f32) -> Self {
        Self {
            light_type: LightType::Point,
            color,
            intensity,
            cast_shadow: false,
        }
    }

    /// Create a spot light.
    pub fn spot(color: Vec3, intensity: f32, _inner_angle: f32, _outer_angle: f32) -> Self {
        Self {
            light_type: LightType::Spot,
            color,
            intensity,
            cast_shadow: true,
        }
    }
}
