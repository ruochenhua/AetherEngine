use aether_engine::physics::PhysicsRuntime;
use std::{fs::OpenOptions, io::Write, path::Path};

pub(super) fn write_report(
    path: &Path,
    scene: &str,
    simulation_time_seconds: f64,
    runtime: &PhysicsRuntime,
) -> anyhow::Result<()> {
    let debug_frame = runtime.debug_frame();
    let allocation = runtime.allocation_stats();
    let report = serde_json::json!({
        "schema_version": 1,
        "scene": scene,
        "simulation_time_seconds": simulation_time_seconds,
        "runtime": {
            "body_count": runtime.body_count(),
            "dynamic_body_count": runtime.dynamic_body_count(),
            "collider_count": runtime.collider_count(),
            "state_sha256": runtime.state_sha256(),
            "allocation": {
                "body_handles_created": allocation.body_handles_created,
                "collider_handles_created": allocation.collider_handles_created,
            },
            "debug": {
                "enabled": debug_frame.is_some(),
                "frame_id": debug_frame.as_ref().map(|frame| frame.frame_id),
                "segment_count": debug_frame.as_ref().map_or(0, |frame| frame.lines.len()),
            },
        },
    });
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(&serde_json::to_vec_pretty(&report)?)?;
    file.write_all(b"\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::write_report;
    use aether_engine::{
        ecs::components::Transform,
        physics::{Collider, ColliderShape, PhysicsRuntime, RigidBody},
    };
    use glam::Vec3;

    #[test]
    fn writes_typed_runtime_counts_hash_and_debug_summary() {
        let mut runtime = PhysicsRuntime::default();
        runtime
            .spawn_body(
                42,
                &Transform::default(),
                &RigidBody::default(),
                &Collider {
                    shape: ColliderShape::Box(Vec3::splat(0.5)),
                    ..Collider::default()
                },
            )
            .unwrap();
        runtime.set_debug_enabled(true);
        let path = std::env::temp_dir().join(format!(
            "aether-physics-report-{}-{}.json",
            std::process::id(),
            runtime.state_sha256()
        ));

        write_report(&path, "t6_fixture", 10.0, &runtime).unwrap();
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        std::fs::remove_file(path).unwrap();

        assert_eq!(report["schema_version"], 1);
        assert_eq!(report["scene"], "t6_fixture");
        assert_eq!(report["simulation_time_seconds"], 10.0);
        assert_eq!(report["runtime"]["body_count"], 1);
        assert_eq!(report["runtime"]["dynamic_body_count"], 1);
        assert_eq!(report["runtime"]["collider_count"], 1);
        assert_eq!(report["runtime"]["allocation"]["body_handles_created"], 1);
        assert_eq!(
            report["runtime"]["allocation"]["collider_handles_created"],
            1
        );
        assert_eq!(
            report["runtime"]["state_sha256"].as_str().unwrap().len(),
            64
        );
        assert_eq!(report["runtime"]["debug"]["enabled"], true);
        assert!(
            report["runtime"]["debug"]["segment_count"]
                .as_u64()
                .unwrap()
                > 0
        );
    }
}
