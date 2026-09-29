//! Convert physics wireframe snapshots into the renderer's line-list vertices.

use aether_engine::{physics::PhysicsDebugFrame, renderer::passes::debug::DebugVertex};

pub(super) fn append_lines(vertices: &mut Vec<DebugVertex>, frame: &PhysicsDebugFrame) {
    vertices.reserve(frame.lines.len() * 2);
    for line in &frame.lines {
        vertices.push(DebugVertex {
            position: line.start.to_array(),
            color: line.color,
        });
        vertices.push(DebugVertex {
            position: line.end.to_array(),
            color: line.color,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn each_physics_segment_becomes_two_colored_vertices() {
        let mut vertices = Vec::new();
        append_lines(
            &mut vertices,
            &PhysicsDebugFrame {
                frame_id: 7,
                lines: vec![aether_engine::physics::DebugLine {
                    start: Vec3::X,
                    end: Vec3::Y,
                    color: [0.2, 0.4, 0.8, 1.0],
                }],
            },
        );
        assert_eq!(vertices.len(), 2);
        assert_eq!(vertices[0].position, Vec3::X.to_array());
        assert_eq!(vertices[1].position, Vec3::Y.to_array());
        assert_eq!(vertices[0].color, [0.2, 0.4, 0.8, 1.0]);
        assert_eq!(vertices[1].color, [0.2, 0.4, 0.8, 1.0]);
    }
}
