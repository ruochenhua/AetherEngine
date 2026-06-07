use winit::event::{ElementState, MouseButton, WindowEvent};

/// Input state manager.
///
/// Tracks keyboard and mouse input for the current frame.
#[derive(Debug, Default)]
pub struct InputManager {
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
            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        state,
                        physical_key: winit::keyboard::PhysicalKey::Code(keycode),
                        ..
                    },
                ..
            } => {
                match state {
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
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let new_pos = (position.x as f32, position.y as f32);
                self.mouse_delta.0 += new_pos.0 - self.mouse_position.0;
                self.mouse_delta.1 += new_pos.1 - self.mouse_position.1;
                self.mouse_position = new_pos;
            }
            WindowEvent::MouseInput { state, button, .. } => {
                match state {
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
                }
            }
            _ => {}
        }
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
        self.key_held(winit::keyboard::KeyCode::AltLeft)
            || self.key_held(winit::keyboard::KeyCode::AltRight)
    }

    /// Check if Ctrl is being held.
    pub fn ctrl_held(&self) -> bool {
        self.key_held(winit::keyboard::KeyCode::ControlLeft)
            || self.key_held(winit::keyboard::KeyCode::ControlRight)
    }

    /// Check if Shift is being held.
    pub fn shift_held(&self) -> bool {
        self.key_held(winit::keyboard::KeyCode::ShiftLeft)
            || self.key_held(winit::keyboard::KeyCode::ShiftRight)
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
