//! Window management abstraction.
//!
//! Thin wrapper around `winit` window creation.

use winit::window::Window;

/// Window manager handle.
pub struct WindowManager {
    window: Window,
}

impl WindowManager {
    /// Get the underlying winit window.
    pub fn window(&self) -> &Window {
        &self.window
    }

    /// Request a redraw.
    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    /// Get the window inner size.
    pub fn inner_size(&self) -> (u32, u32) {
        let size = self.window.inner_size();
        (size.width, size.height)
    }
}
