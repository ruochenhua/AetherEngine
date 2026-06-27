//! Camera configuration.

use serde::{Deserialize, Serialize};

/// FlyCamera initial parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CameraConfig {
    /// World-space starting position [x, y, z].
    pub position: [f32; 3],
    /// Initial yaw angle in radians.
    #[serde(default)]
    pub yaw: f32,
    /// Initial pitch angle in radians.
    #[serde(default)]
    pub pitch: f32,
    /// Movement speed (units per second).
    #[serde(default = "default_camera_speed")]
    pub speed: f32,
    /// Vertical field of view in degrees.
    #[serde(default = "default_fov")]
    pub fov: f32,
    /// Near clip plane.
    #[serde(default = "default_near")]
    pub near: f32,
    /// Far clip plane.
    #[serde(default = "default_far")]
    pub far: f32,
}

fn default_camera_speed() -> f32 {
    4.0
}
fn default_fov() -> f32 {
    45.0
}
fn default_near() -> f32 {
    0.1
}
fn default_far() -> f32 {
    1000.0
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            position: [3.0, 3.0, 3.0],
            yaw: -2.356,
            pitch: -0.785,
            speed: default_camera_speed(),
            fov: default_fov(),
            near: default_near(),
            far: default_far(),
        }
    }
}
