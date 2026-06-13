use crate::math::*;

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
        Mat4::look_at_rh(self.position, self.position + self.forward(), Vec3::Y)
    }

    /// Projection matrix.
    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov, aspect, self.near, self.far)
    }

    /// Compute the world-space view frustum for this camera.
    pub fn frustum_planes(&self, aspect: f32) -> Frustum {
        let view = self.view_matrix();
        let proj = self.projection_matrix(aspect);
        Frustum::from_view_projection(proj * view)
    }

    /// Update camera from input state.
    ///
    /// `dt` is frame delta time in seconds. `input` provides mouse/key state.
    /// Hold Alt + left mouse + drag to rotate. WASD to move.
    pub fn update(
        &mut self,
        dt: f32,
        mouse_dx: f32,
        mouse_dy: f32,
        scroll: f32,
        input: &crate::input::InputManager,
    ) {
        // Speed adjustment via scroll wheel
        if scroll != 0.0 {
            self.speed =
                (self.speed + scroll * self.base_speed * 0.5).clamp(self.min_speed, self.max_speed);
        }

        // Mouse look only when Alt + left mouse is held and mouse moved
        if input.alt_held()
            && input.mouse_held(winit::event::MouseButton::Left)
            && (mouse_dx.abs() > 0.0 || mouse_dy.abs() > 0.0)
        {
            self.yaw -= mouse_dx * self.sensitivity;
            self.pitch = (self.pitch + mouse_dy * self.sensitivity).clamp(
                -std::f32::consts::FRAC_PI_2 + 0.01,
                std::f32::consts::FRAC_PI_2 - 0.01,
            );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_frustum_planes_perspective() {
        let camera = FlyCamera {
            position: Vec3::new(0.0, 0.0, 5.0),
            // yaw = π makes the camera look toward -X.
            yaw: std::f32::consts::PI,
            pitch: 0.0,
            fov: 90.0f32.to_radians(),
            near: 0.1,
            far: 100.0,
            ..Default::default()
        };

        let frustum = camera.frustum_planes(1.0);

        // Camera looks toward -X. The outward left-plane normal has a +Z component
        // (camera's left side is +Z) and a +X component.
        assert!(frustum.planes[Frustum::LEFT].normal.z > 0.0);
        // The outward right-plane normal has a -Z component.
        assert!(frustum.planes[Frustum::RIGHT].normal.z < 0.0);
        // Top plane normal points +Y.
        assert!(frustum.planes[Frustum::TOP].normal.y > 0.0);
        // Bottom plane normal points -Y.
        assert!(frustum.planes[Frustum::BOTTOM].normal.y < 0.0);
        // Near plane is slightly in front of the camera; outward normal points +X.
        assert!(frustum.planes[Frustum::NEAR].normal.x > 0.0);
        // Far plane is behind the scene; outward normal points -X.
        assert!(frustum.planes[Frustum::FAR].normal.x < 0.0);
    }

    #[test]
    fn camera_frustum_planes_orthographic() {
        let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        let proj = Mat4::orthographic_rh(-4.0, 4.0, -3.0, 3.0, 0.1, 100.0);
        let frustum = Frustum::from_view_projection(proj * view);

        // Orthographic left/right normals are parallel to world X.
        assert!(frustum.planes[Frustum::LEFT]
            .normal
            .abs_diff_eq(Vec3::NEG_X, 1e-3));
        assert!(frustum.planes[Frustum::RIGHT]
            .normal
            .abs_diff_eq(Vec3::X, 1e-3));
        assert!(frustum.planes[Frustum::BOTTOM]
            .normal
            .abs_diff_eq(Vec3::NEG_Y, 1e-3));
        assert!(frustum.planes[Frustum::TOP]
            .normal
            .abs_diff_eq(Vec3::Y, 1e-3));
        // Near points toward camera (+Z from world origin, camera at z=5 looks at origin so near normal is +Z).
        assert!(frustum.planes[Frustum::NEAR].normal.z > 0.0);
        assert!(frustum.planes[Frustum::FAR].normal.z < 0.0);
    }

    #[test]
    fn perspective_frustum_culls_object_behind_camera() {
        let camera = FlyCamera {
            position: Vec3::new(0.0, 0.0, 5.0),
            yaw: std::f32::consts::PI,
            pitch: 0.0,
            fov: 90.0f32.to_radians(),
            near: 0.1,
            far: 100.0,
            ..Default::default()
        };

        let frustum = camera.frustum_planes(1.0);

        // Object behind the camera (positive X) should be invisible.
        let behind = Aabb::from_min_max(Vec3::new(10.0, -1.0, -1.0), Vec3::new(11.0, 1.0, 1.0));
        assert_eq!(
            behind.intersects_frustum(&frustum),
            CullingVisibility::Invisible
        );

        // Object in front of the camera (negative X) should be at least partially visible.
        let in_front = Aabb::from_min_max(Vec3::new(-5.0, -1.0, -1.0), Vec3::new(-4.0, 1.0, 1.0));
        let vis = in_front.intersects_frustum(&frustum);
        assert!(vis == CullingVisibility::Visible || vis == CullingVisibility::Partial);
    }
}
