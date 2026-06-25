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
}

fn default_camera_speed() -> f32 {
    4.0
}
fn default_fov() -> f32 {
    45.0
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            position: [3.0, 3.0, 3.0],
            yaw: -2.356,
            pitch: -0.785,
            speed: default_camera_speed(),
            fov: default_fov(),
        }
    }
}
