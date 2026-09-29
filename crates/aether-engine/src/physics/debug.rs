//! Deterministic wireframe extraction for physics colliders.

use super::{runtime::PhysicsRuntime, types::DebugLine, PhysicsDebugFrame};
use crate::physics::components::ColliderShape;
use glam::Vec3;

const BOX_COLOR: [f32; 4] = [1.0, 0.78, 0.12, 1.0];
const SPHERE_COLOR: [f32; 4] = [0.12, 0.88, 1.0, 1.0];
const SPHERE_SEGMENTS: usize = 24;

pub(super) fn extract_debug_frame(runtime: &PhysicsRuntime) -> Option<PhysicsDebugFrame> {
    if !runtime.debug_enabled {
        return None;
    }

    let mut frame = PhysicsDebugFrame {
        frame_id: runtime.last_step_index,
        lines: Vec::new(),
    };
    for state in runtime.entity_state.values() {
        let Some(body) = runtime.bodies.get(state.body_handle) else {
            continue;
        };
        let translation = body.translation();
        let rotation = body.rotation();
        let origin = Vec3::new(translation.x, translation.y, translation.z);
        for collider in &state.desc.colliders {
            match &collider.shape {
                ColliderShape::Box(half_extents) => {
                    append_box(
                        &mut frame.lines,
                        origin,
                        rotation,
                        *half_extents * state.scale,
                    );
                }
                ColliderShape::Sphere(radius) => {
                    append_sphere(&mut frame.lines, origin, rotation, *radius * state.scale.x);
                }
                ColliderShape::Capsule(_, _) | ColliderShape::Mesh => {}
            }
        }
    }
    Some(frame)
}

fn transform_point(origin: Vec3, rotation: &rapier3d::na::UnitQuaternion<f32>, p: Vec3) -> Vec3 {
    let transformed = rotation * rapier3d::na::Vector3::new(p.x, p.y, p.z);
    origin + Vec3::new(transformed.x, transformed.y, transformed.z)
}

fn append_box(
    lines: &mut Vec<DebugLine>,
    origin: Vec3,
    rotation: &rapier3d::na::UnitQuaternion<f32>,
    half: Vec3,
) {
    for corner in 0..8 {
        let start = Vec3::new(
            if corner & 1 == 0 { -half.x } else { half.x },
            if corner & 2 == 0 { -half.y } else { half.y },
            if corner & 4 == 0 { -half.z } else { half.z },
        );
        for axis in [1, 2, 4] {
            if corner & axis == 0 {
                let end = match axis {
                    1 => Vec3::new(half.x, start.y, start.z),
                    2 => Vec3::new(start.x, half.y, start.z),
                    _ => Vec3::new(start.x, start.y, half.z),
                };
                lines.push(DebugLine {
                    start: transform_point(origin, rotation, start),
                    end: transform_point(origin, rotation, end),
                    color: BOX_COLOR,
                });
            }
        }
    }
}

fn append_sphere(
    lines: &mut Vec<DebugLine>,
    origin: Vec3,
    rotation: &rapier3d::na::UnitQuaternion<f32>,
    radius: f32,
) {
    for (axis_a, axis_b) in [(0, 1), (0, 2), (1, 2)] {
        for segment in 0..SPHERE_SEGMENTS {
            let angle0 = std::f32::consts::TAU * segment as f32 / SPHERE_SEGMENTS as f32;
            let angle1 = std::f32::consts::TAU * (segment + 1) as f32 / SPHERE_SEGMENTS as f32;
            let mut start = Vec3::ZERO;
            let mut end = Vec3::ZERO;
            start[axis_a] = radius * angle0.cos();
            start[axis_b] = radius * angle0.sin();
            end[axis_a] = radius * angle1.cos();
            end[axis_b] = radius * angle1.sin();
            lines.push(DebugLine {
                start: transform_point(origin, rotation, start),
                end: transform_point(origin, rotation, end),
                color: SPHERE_COLOR,
            });
        }
    }
}
