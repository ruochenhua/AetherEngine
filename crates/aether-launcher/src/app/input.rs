//! Per-frame input handling: debug hotkeys, camera movement, picking and gizmos.

mod camera_picking;
mod physics_picking;
pub(crate) use camera_picking::update_camera_and_picking;

use super::App;
use winit::keyboard::KeyCode;

/// Process number / function-key debug overlays.
pub(crate) fn process_debug_hotkeys(app: &mut App) {
    // Only process debug hotkeys when egui is not capturing keyboard input
    // (e.g. when typing in an Inspector text field).
    if app.egui_ctx.egui_wants_keyboard_input() {
        return;
    }

    if app.input.key_pressed(KeyCode::Digit0) {
        app.debug_mode = 0;
    }
    if app.input.key_pressed(KeyCode::Digit1) {
        app.debug_mode = 1;
    }
    if app.input.key_pressed(KeyCode::Digit2) {
        app.debug_mode = 2;
    }
    if app.input.key_pressed(KeyCode::Digit3) {
        app.debug_mode = 3;
    }
    if app.input.key_pressed(KeyCode::Digit4) {
        app.debug_mode = 4;
    }
    if app.input.key_pressed(KeyCode::Digit5) {
        app.debug_mode = 5;
    }
    if app.input.key_pressed(KeyCode::Digit6) {
        app.debug_mode = 6;
    }
    if app.input.key_pressed(KeyCode::Digit7) {
        app.debug_mode = 7;
    }
    if app.input.key_pressed(KeyCode::Digit8) {
        app.debug_mode = 8;
    }
    if app.input.key_pressed(KeyCode::Digit9) {
        app.debug_mode = 9;
    }
    if app.input.key_pressed(KeyCode::F1) {
        app.debug_mode = 10;
    }
    if app.input.key_pressed(KeyCode::F2) {
        app.debug_mode = 11;
    }
    if app.input.key_pressed(KeyCode::F3) {
        app.debug_mode = 12;
    }
    if app.input.key_pressed(KeyCode::F4) {
        app.debug_mode = 13;
    }
    if app.input.key_pressed(KeyCode::F5) {
        app.debug_mode = 14;
    }
    if app.input.key_pressed(KeyCode::F6) {
        app.ssr_debug_mode = (app.ssr_debug_mode + 1) % 10;
    }
    if app.input.key_pressed(KeyCode::F7) {
        app.debug_mode = 15;
    }
}
