use crate::math::*;

/// Camera component.
#[derive(Debug, Clone)]
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

/// UE-style fly camera.
///
/// Right-click to activate: mouse controls pitch/yaw, WASD moves along
/// camera-relative axes, Q/E moves up/down in world space, scroll wheel
/// adjusts movement speed.
#[derive(Debug, Clone)]
pub struct FlyCamera {
    /// World-space position.
    pub position: Vec3,
    /// Yaw angle in radians (rotation around world Y axis).
    pub yaw: f32,
    /// Pitch angle in radians (rotation around local X axis). Clamped to (-π/2, π/2).
    pub pitch: f32,
    /// Current movement speed (units per second).
    pub speed: f32,
    /// Base movement speed before scroll adjustment.
    pub base_speed: f32,
    /// Minimum speed.
    pub min_speed: f32,
    /// Maximum speed.
    pub max_speed: f32,
    /// Mouse sensitivity for look rotation.
    pub sensitivity: f32,
    /// Whether fly mode is active (right mouse button held).
    pub active: bool,
    /// Vertical field of view in radians.
    pub fov: f32,
    /// Near clip plane.
    pub near: f32,
    /// Far clip plane.
    pub far: f32,
}

impl Default for FlyCamera {
    fn default() -> Self {
        Self {
            position: Vec3::new(3.0, 3.0, 3.0),
            yaw: -std::f32::consts::FRAC_PI_4 - std::f32::consts::FRAC_PI_2,
            pitch: -std::f32::consts::FRAC_PI_4,
            speed: 4.0,
            base_speed: 4.0,
            min_speed: 0.1,
            max_speed: 100.0,
            sensitivity: 0.002,
            active: false,
            fov: 45.0f32.to_radians(),
            near: 0.1,
            far: 1000.0,
        }
    }
}

impl FlyCamera {
    /// Forward direction (look direction).
    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        )
    }

    /// Right direction.
    pub fn right(&self) -> Vec3 {
        let fwd = self.forward();
        fwd.cross(Vec3::Y).normalize()
    }

    /// Camera-local up direction.
    pub fn cam_up(&self) -> Vec3 {
        self.right().cross(self.forward()).normalize()
    }

    /// View matrix.
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(
            self.position,
            self.position + self.forward(),
            Vec3::Y,
        )
    }

    /// Projection matrix.
    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov, aspect, self.near, self.far)
    }

    /// Update camera from input state.
    ///
    /// `dt` is frame delta time in seconds. `input` provides mouse/key state.
    /// The caller should toggle `active` via `input.mouse_pressed(Right)`
    /// before calling this method.
    pub fn update(&mut self, dt: f32, mouse_dx: f32, mouse_dy: f32, scroll: f32, input: &crate::input::InputManager) {
        // Toggle fly mode on right-click
        if input.mouse_pressed(winit::event::MouseButton::Right) {
            self.active = !self.active;
        }

        // Speed adjustment via scroll wheel (only when active)
        if self.active && scroll != 0.0 {
            self.speed = (self.speed + scroll * self.base_speed * 0.5)
                .clamp(self.min_speed, self.max_speed);
        }

        if !self.active {
            return;
        }

        // Mouse look
        if mouse_dx != 0.0 || mouse_dy != 0.0 {
            self.yaw -= mouse_dx * self.sensitivity;
            self.pitch = (self.pitch + mouse_dy * self.sensitivity)
                .clamp(-std::f32::consts::FRAC_PI_2 + 0.01, std::f32::consts::FRAC_PI_2 - 0.01);
        }

        // WASD movement along camera axes
        let fwd = self.forward();
        let right = self.right();
        let step = self.speed * dt;

        if input.key_held(winit::keyboard::KeyCode::KeyW) {
            self.position += fwd * step;
        }
        if input.key_held(winit::keyboard::KeyCode::KeyS) {
            self.position -= fwd * step;
        }
        if input.key_held(winit::keyboard::KeyCode::KeyA) {
            self.position -= right * step;
        }
        if input.key_held(winit::keyboard::KeyCode::KeyD) {
            self.position += right * step;
        }

        // Q/E world-space up/down
        if input.key_held(winit::keyboard::KeyCode::KeyE) {
            self.position.y += step;
        }
        if input.key_held(winit::keyboard::KeyCode::KeyQ) {
            self.position.y -= step;
        }
    }
}
