use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::keyboard::ModifiersState;

/// Input state manager.
///
/// Tracks keyboard and mouse input for the current frame.
#[derive(Debug, Default)]
pub struct InputManager {
    modifiers: ModifiersState,
    keys_pressed: Vec<winit::keyboard::KeyCode>,
    keys_held: Vec<winit::keyboard::KeyCode>,
    keys_released: Vec<winit::keyboard::KeyCode>,
    mouse_position: (f32, f32),
    mouse_delta: (f32, f32),
    mouse_buttons_pressed: Vec<MouseButton>,
    mouse_buttons_held: Vec<MouseButton>,
    mouse_buttons_released: Vec<MouseButton>,
}

impl InputManager {
    /// Create a new input manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Handle a window event and update input state.
    pub fn handle_window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::Focused(false) => self.clear_held_state(),
            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        state,
                        physical_key: winit::keyboard::PhysicalKey::Code(keycode),
                        ..
                    },
                ..
            } => match state {
                ElementState::Pressed => {
                    if !self.keys_held.contains(keycode) {
                        self.keys_pressed.push(*keycode);
                        self.keys_held.push(*keycode);
                    }
                }
                ElementState::Released => {
                    self.keys_held.retain(|k| k != keycode);
                    self.keys_released.push(*keycode);
                }
            },
            WindowEvent::CursorMoved { position, .. } => {
                let new_pos = (position.x as f32, position.y as f32);
                self.mouse_delta.0 += new_pos.0 - self.mouse_position.0;
                self.mouse_delta.1 += new_pos.1 - self.mouse_position.1;
                self.mouse_position = new_pos;
            }
            WindowEvent::MouseInput { state, button, .. } => match state {
                ElementState::Pressed => {
                    if !self.mouse_buttons_held.contains(button) {
                        self.mouse_buttons_pressed.push(*button);
                        self.mouse_buttons_held.push(*button);
                    }
                }
                ElementState::Released => {
                    self.mouse_buttons_held.retain(|b| b != button);
                    self.mouse_buttons_released.push(*button);
                }
            },
            _ => {}
        }
    }

    /// Clear held input after focus loss so a missing release event cannot stick.
    pub fn clear_held_state(&mut self) {
        self.modifiers = ModifiersState::empty();
        self.keys_pressed.clear();
        self.keys_held.clear();
        self.keys_released.clear();
        self.mouse_buttons_pressed.clear();
        self.mouse_buttons_held.clear();
        self.mouse_buttons_released.clear();
        self.mouse_delta = (0.0, 0.0);
    }

    /// Check if a key was pressed this frame.
    pub fn key_pressed(&self, key: winit::keyboard::KeyCode) -> bool {
        self.keys_pressed.contains(&key)
    }

    /// Check if a key is being held.
    pub fn key_held(&self, key: winit::keyboard::KeyCode) -> bool {
        self.keys_held.contains(&key)
    }

    /// Check if a mouse button was pressed this frame.
    pub fn mouse_pressed(&self, button: MouseButton) -> bool {
        self.mouse_buttons_pressed.contains(&button)
    }

    /// Get the current mouse position.
    pub fn mouse_position(&self) -> (f32, f32) {
        self.mouse_position
    }

    /// Get the mouse delta since last frame.
    pub fn mouse_delta(&self) -> (f32, f32) {
        self.mouse_delta
    }

    /// Check if a mouse button is being held.
    pub fn mouse_held(&self, button: MouseButton) -> bool {
        self.mouse_buttons_held.contains(&button)
    }

    /// Check if a mouse button was released this frame.
    pub fn mouse_released(&self, button: MouseButton) -> bool {
        self.mouse_buttons_released.contains(&button)
    }

    /// Check if Alt is being held.
    pub fn alt_held(&self) -> bool {
        self.modifiers.alt_key()
    }

    /// Check if Ctrl (or Cmd on macOS) is being held.
    pub fn ctrl_held(&self) -> bool {
        self.modifiers.control_key() || self.modifiers.super_key()
    }

    /// Check if Shift is being held.
    pub fn shift_held(&self) -> bool {
        self.modifiers.shift_key()
    }

    /// Clear per-frame input state (pressed/released events).
    pub fn end_frame(&mut self) {
        self.keys_pressed.clear();
        self.keys_released.clear();
        self.mouse_buttons_pressed.clear();
        self.mouse_buttons_released.clear();
        self.mouse_delta = (0.0, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::camera::FlyCamera;

    #[test]
    fn losing_window_focus_clears_held_keys_buttons_and_modifiers() {
        let mut input = InputManager::default();
        input.keys_held.push(winit::keyboard::KeyCode::KeyW);
        input.mouse_buttons_held.push(MouseButton::Right);
        input.modifiers = ModifiersState::ALT | ModifiersState::SHIFT;

        input.handle_window_event(&WindowEvent::Focused(false));

        assert!(!input.key_held(winit::keyboard::KeyCode::KeyW));
        assert!(!input.mouse_held(MouseButton::Right));
        assert!(!input.alt_held());
        assert!(!input.shift_held());
    }

    #[test]
    fn fly_movement_requires_an_accepted_navigation_gesture() {
        let mut camera = FlyCamera::default();
        let mut input = InputManager::default();
        input.keys_held.push(winit::keyboard::KeyCode::KeyW);
        let start = camera.position;

        camera.update(1.0, 0.0, 0.0, 0.0, &input);

        assert_eq!(camera.position, start);
    }

    #[test]
    fn left_mouse_held_with_wasd_flies_camera() {
        let mut camera = FlyCamera::default();
        let mut input = InputManager::default();
        input.mouse_buttons_held.push(MouseButton::Left);
        input.keys_held.push(winit::keyboard::KeyCode::KeyW);
        let start = camera.position;

        camera.update(1.0, 0.0, 0.0, 0.0, &input);

        assert_ne!(camera.position, start);
    }

    #[test]
    fn left_mouse_click_without_fly_keys_does_not_move_or_rotate_camera() {
        let mut camera = FlyCamera::default();
        let mut input = InputManager::default();
        input.mouse_buttons_held.push(MouseButton::Left);
        let start = camera.position;
        let start_yaw = camera.yaw;

        camera.update(1.0, 20.0, 10.0, 0.0, &input);

        assert_eq!(camera.position, start);
        assert_eq!(camera.yaw, start_yaw);
    }

    #[test]
    fn left_mouse_drag_rotates_the_view() {
        let mut camera = FlyCamera::default();
        let mut input = InputManager::default();
        input.mouse_buttons_held.push(MouseButton::Left);
        camera.active = true;
        let start_yaw = camera.yaw;

        camera.update(1.0, 20.0, 0.0, 0.0, &input);

        assert_ne!(camera.yaw, start_yaw);
    }

    #[test]
    fn left_mouse_button_without_viewport_activation_does_not_rotate() {
        let mut camera = FlyCamera::default();
        let mut input = InputManager::default();
        input.mouse_buttons_held.push(MouseButton::Left);
        let start_yaw = camera.yaw;

        camera.update(1.0, 20.0, 0.0, 0.0, &input);

        assert_eq!(camera.yaw, start_yaw);
        assert!(!camera.active);
    }

    #[test]
    fn scroll_dollies_camera_without_changing_fly_speed() {
        let mut camera = FlyCamera::default();
        let start_position = camera.position;
        let start_speed = camera.speed;

        camera.update(0.0, 0.0, 0.0, 1.0, &InputManager::default());

        assert_ne!(camera.position, start_position);
        assert_eq!(camera.speed, start_speed);
    }

    #[test]
    fn trackpad_dolly_still_works_while_keyboard_input_is_owned_by_ui() {
        let mut camera = FlyCamera::default();
        let mut input = InputManager::default();
        input.keys_held.push(winit::keyboard::KeyCode::KeyW);
        let start_position = camera.position;

        camera.update_with_keyboard(1.0, 0.0, 0.0, 1.0, &input, false);

        assert_ne!(camera.position, start_position);
        let dolly_position = camera.position;
        camera.update_with_keyboard(1.0, 0.0, 0.0, 0.0, &input, false);
        assert_eq!(camera.position, dolly_position);
    }

    #[test]
    fn scroll_during_left_mouse_navigation_dollies_camera() {
        let mut camera = FlyCamera {
            active: true,
            ..FlyCamera::default()
        };
        let mut input = InputManager::default();
        input.mouse_buttons_held.push(MouseButton::Left);
        let start_position = camera.position;
        let start_speed = camera.speed;

        camera.update(0.0, 0.0, 0.0, 1.0, &input);

        assert_ne!(camera.position, start_position);
        assert_eq!(camera.speed, start_speed);
    }

    #[test]
    fn middle_mouse_drag_pans_without_rotating() {
        let mut camera = FlyCamera::default();
        let mut input = InputManager::default();
        input.mouse_buttons_held.push(MouseButton::Middle);
        let start_position = camera.position;
        let start_yaw = camera.yaw;

        camera.update(0.0, 20.0, 10.0, 0.0, &input);

        assert_ne!(camera.position, start_position);
        assert_eq!(camera.yaw, start_yaw);
    }

    #[test]
    fn shift_scroll_pans_vertically_instead_of_dollying() {
        let mut camera = FlyCamera::default();
        let input = InputManager {
            modifiers: ModifiersState::SHIFT,
            ..InputManager::default()
        };
        let start_position = camera.position;
        let start_forward = camera.forward();

        camera.update(0.0, 0.0, 0.0, 1.0, &input);

        let movement = camera.position - start_position;
        assert!(movement.dot(start_forward).abs() < 1e-5);
        assert!(movement.length() > 0.0);
    }
}
