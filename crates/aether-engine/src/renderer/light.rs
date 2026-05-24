use glam::Vec3;

/// Light type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
