use crate::math::*;
use crate::ecs::Component;

/// Camera component.
#[derive(Debug, Clone, Component)]
pub struct Camera {
    /// Vertical field of view in radians.
    pub fov: f32,
    /// Near clip plane.
    pub near: f32,
    /// Far clip plane.
    pub far: f32,
    /// Aspect ratio (width / height).
    pub aspect: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            fov: 45.0f32.to_radians(),
            near: 0.1,
            far: 1000.0,
            aspect: 16.0 / 9.0,
        }
    }
}

impl Camera {
    /// Create a perspective camera.
    pub fn perspective(fov_degrees: f32, aspect: f32, near: f32, far: f32) -> Self {
        Self {
            fov: fov_degrees.to_radians(),
            near,
            far,
            aspect,
        }
    }

    /// Get the projection matrix.
    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(self.fov, self.aspect, self.near, self.far)
    }

    /// Get the view matrix from a transform.
    pub fn view_matrix(&self, position: Vec3, forward: Vec3, up: Vec3) -> Mat4 {
        Mat4::look_at_rh(position, position + forward, up)
    }
}

/// Orbit camera controller.
#[derive(Debug, Clone, Component)]
pub struct OrbitCamera {
    /// Distance from target.
    pub distance: f32,
    /// Azimuth angle (horizontal rotation).
    pub azimuth: f32,
    /// Polar angle (vertical rotation).
    pub polar: f32,
    /// Target point to orbit around.
    pub target: Vec3,
    /// Mouse sensitivity.
    pub sensitivity: f32,
    /// Zoom sensitivity.
    pub zoom_sensitivity: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            distance: 5.0,
            azimuth: 0.0,
            polar: std::f32::consts::FRAC_PI_4,
            target: Vec3::ZERO,
            sensitivity: 0.01,
            zoom_sensitivity: 0.1,
        }
    }
}

impl OrbitCamera {
    /// Update the orbit camera from mouse input.
    pub fn update(&mut self, delta_x: f32, delta_y: f32, scroll: f32) {
        if delta_x != 0.0 {
            self.azimuth -= delta_x * self.sensitivity;
        }
        if delta_y != 0.0 {
            self.polar = (self.polar - delta_y * self.sensitivity)
                .clamp(0.01, std::f32::consts::PI - 0.01);
        }
        if scroll != 0.0 {
            self.distance = (self.distance - scroll * self.zoom_sensitivity)
                .clamp(0.1, 1000.0);
        }
    }

    /// Get the camera position in world space.
    pub fn position(&self) -> Vec3 {
        let x = self.distance * self.polar.sin() * self.azimuth.cos();
        let y = self.distance * self.polar.cos();
        let z = self.distance * self.polar.sin() * self.azimuth.sin();
        self.target + Vec3::new(x, y, z)
    }

    /// Get the forward direction.
    pub fn forward(&self) -> Vec3 {
        (self.target - self.position()).normalize()
    }

    /// Get the up direction.
    pub fn up(&self) -> Vec3 {
        Vec3::Y
    }

    /// Get the right direction.
    pub fn right(&self) -> Vec3 {
        self.forward().cross(self.up()).normalize()
    }
}
