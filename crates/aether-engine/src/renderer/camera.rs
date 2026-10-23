use crate::math::*;

/// UE-style fly camera.
///
/// Left-click drag looks around; left-click + WASD flies. Scroll moves along
/// the view direction. Shift + scroll pans vertically.
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
    /// Whether the viewport accepted the active left-button navigation gesture.
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
        look_at(self.position, self.position + self.forward(), Vec3::Y)
    }

    /// Projection matrix.
    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        perspective_projection(self.fov, aspect, self.near, self.far)
    }

    /// Compute the world-space view frustum for this camera.
    pub fn frustum_planes(&self, aspect: f32) -> Frustum {
        let view = self.view_matrix();
        let proj = self.projection_matrix(aspect);
        Frustum::from_view_projection(proj * view)
    }

    /// Update camera from input state. Left mouse activates look and fly controls.
    pub fn update(
        &mut self,
        dt: f32,
        mouse_dx: f32,
        mouse_dy: f32,
        scroll: f32,
        input: &crate::input::InputManager,
    ) {
        self.update_with_keyboard(dt, mouse_dx, mouse_dy, scroll, input, true);
    }

    /// Update pointer navigation while optionally ignoring keyboard movement.
    /// This keeps viewport gestures available when an egui text field owns keys.
    pub fn update_with_keyboard(
        &mut self,
        dt: f32,
        mouse_dx: f32,
        mouse_dy: f32,
        scroll: f32,
        input: &crate::input::InputManager,
        keyboard_enabled: bool,
    ) {
        let left_mouse_navigation =
            self.active && input.mouse_held(winit::event::MouseButton::Left);
        let left_mouse_keyboard_navigation = keyboard_enabled
            && input.mouse_held(winit::event::MouseButton::Left)
            && [
                winit::keyboard::KeyCode::KeyW,
                winit::keyboard::KeyCode::KeyA,
                winit::keyboard::KeyCode::KeyS,
                winit::keyboard::KeyCode::KeyD,
                winit::keyboard::KeyCode::KeyQ,
                winit::keyboard::KeyCode::KeyE,
            ]
            .into_iter()
            .any(|key| input.key_held(key));
        let navigating = left_mouse_navigation || left_mouse_keyboard_navigation;

        if left_mouse_navigation && (mouse_dx.abs() > 0.0 || mouse_dy.abs() > 0.0) {
            self.yaw -= mouse_dx * self.sensitivity;
            self.pitch = (self.pitch + mouse_dy * self.sensitivity).clamp(
                -std::f32::consts::FRAC_PI_2 + 0.01,
                std::f32::consts::FRAC_PI_2 - 0.01,
            );
        }

        if scroll != 0.0 {
            if input.shift_held() {
                self.position += self.cam_up() * scroll * self.speed * 0.5;
            } else {
                self.position += self.forward() * scroll * self.speed * 0.5;
            }
        }

        if input.mouse_held(winit::event::MouseButton::Middle)
            && (mouse_dx.abs() > 0.0 || mouse_dy.abs() > 0.0)
        {
            let pan_scale = self.speed * 0.01;
            self.position -= self.right() * mouse_dx * pan_scale;
            self.position += self.cam_up() * mouse_dy * pan_scale;
        }

        // WASD movement is scoped to an active viewport navigation gesture.
        if !navigating {
            return;
        }

        let fwd = self.forward();
        let right = self.right();
        let step = self.speed * dt;

        if keyboard_enabled && input.key_held(winit::keyboard::KeyCode::KeyW) {
            self.position += fwd * step;
        }
        if keyboard_enabled && input.key_held(winit::keyboard::KeyCode::KeyS) {
            self.position -= fwd * step;
        }
        if keyboard_enabled && input.key_held(winit::keyboard::KeyCode::KeyA) {
            self.position -= right * step;
        }
        if keyboard_enabled && input.key_held(winit::keyboard::KeyCode::KeyD) {
            self.position += right * step;
        }

        // Q/E world-space up/down
        if keyboard_enabled && input.key_held(winit::keyboard::KeyCode::KeyE) {
            self.position.y += step;
        }
        if keyboard_enabled && input.key_held(winit::keyboard::KeyCode::KeyQ) {
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
        let view = look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
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
