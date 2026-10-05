use aether_engine::ecs::components::{Light, Name, Transform, Visibility};
use aether_engine::ecs::World;
use aether_engine::editor::{
    capture_snapshot, ComponentRecord, EditorContext, EditorEntitySnapshot, EditorError,
    EditorHistory, EditorOperation, OperationLog, EDITOR_SCHEMA_VERSION,
};
use aether_engine::renderer::light::LightingUniforms;
use aether_engine::scene::{LightConfig, SceneDescription};
use aether_engine::visual_case::{ExpectedResult, VisualManifest};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(1);

struct FixtureRoot(PathBuf);

impl FixtureRoot {
    fn new(label: &str) -> Self {
        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "aether_t1_{label}_{}_{}",
            std::process::id(),
            sequence
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for FixtureRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}

fn validate_case(case_id: &str, metric_names: &[&str], probe_names: &[&str]) {
    let root = project_root();
    let source = fs::read_to_string(root.join(format!("tests/cases/{case_id}.json"))).unwrap();
    let manifest = VisualManifest::from_json_in(&source, &root).unwrap();
    let materialized = manifest.materialize().unwrap();
    assert_eq!(materialized.len(), 1);
    assert_eq!(materialized[0].id, format!("{case_id}__default"));
    match materialized[0].expected_result.as_ref().unwrap() {
        ExpectedResult::Render {
            metrics, probes, ..
        } => {
            assert_eq!(
                metrics
                    .iter()
                    .map(|metric| metric.name.as_str())
                    .collect::<Vec<_>>(),
                metric_names
            );
            assert_eq!(
                probes
                    .iter()
                    .map(|probe| probe.name.as_str())
                    .collect::<Vec<_>>(),
                probe_names
            );
        }
        ExpectedResult::ExpectedError { .. } => panic!("T1 primary case must be a fixture result"),
    }
}

fn write_events(case_id: &str, metrics: Value, probes: Value) {
    let Some(path) = std::env::var_os("AETHER_T1_EDITOR_EVENTS") else {
        return;
    };
    fs::write(
        path,
        serde_json::to_vec_pretty(&json!({
            "case_id": case_id,
            "kind": "Fixture",
            "metrics": metrics,
            "probes": probes
        }))
        .unwrap(),
    )
    .unwrap();
}

fn probe(name: &str, value: bool) -> Value {
    json!({"name": name, "value": {"kind": "Bool", "value": value}})
}

fn named_world(name: &str, with_light: bool) -> (World, hecs::Entity) {
    let mut world = World::new();
    let entity = if with_light {
        world.spawn((
            Transform::default(),
            Visibility(true),
            Light::default(),
            Name(name.into()),
        ))
    } else {
        world.spawn((Transform::default(), Name(name.into())))
    };
    (world, entity)
}

#[test]
fn t1_editor_transaction() {
    const CASE_ID: &str = "t1_editor_transaction";
    validate_case(
        CASE_ID,
        &["operations"],
        &[
            "rollback_world",
            "rollback_history",
            "atomic_scene_save",
            "operation_jsonl_save",
        ],
    );

    let fixture = FixtureRoot::new("transaction");
    let (mut world, entity) = named_world("Before", false);
    let lighting = LightingUniforms::default();
    let baseline_hash = EditorHistory::state_sha256(&world).unwrap();
    let mut history = EditorHistory::default();
    let failed_save = history.apply(
        &mut EditorContext::new(
            &mut world,
            &lighting,
            "T1Transaction",
            &fixture.0.join("missing/scene.ron"),
            &fixture.0.join("missing/operations.jsonl"),
        ),
        EditorOperation::Rename {
            entity_name: "Before".into(),
            new_name: "MustRollback".into(),
        },
    );
    let rollback_world = matches!(failed_save, Err(EditorError::Io { .. }))
        && EditorHistory::state_sha256(&world).unwrap() == baseline_hash
        && world.query_one::<&Name>(entity).get().unwrap().0 == "Before";
    let rollback_history = history.undo_len() == 0
        && history.redo_len() == 0
        && history.operation_log().apply_count() == 0;

    let valid_dir = fixture.0.join("valid");
    fs::create_dir_all(&valid_dir).unwrap();
    let scene_path = valid_dir.join("scene.ron");
    let log_path = valid_dir.join("operations.jsonl");
    history
        .apply(
            &mut EditorContext::new(
                &mut world,
                &lighting,
                "T1Transaction",
                &scene_path,
                &log_path,
            ),
            EditorOperation::Rename {
                entity_name: "Before".into(),
                new_name: "After".into(),
            },
        )
        .unwrap();
    let scene = fs::read_to_string(&scene_path).unwrap();
    let decoded_scene: SceneDescription = ron::from_str(&scene).unwrap();
    let serialized_log = fs::read_to_string(&log_path).unwrap();
    let parsed_log = OperationLog::from_jsonl(&serialized_log).unwrap();
    let atomic_scene_save = decoded_scene.name == "T1Transaction";
    let operation_jsonl_save = parsed_log.apply_count() == 1
        && serialized_log.contains("rename")
        && serialized_log.contains("After");

    let probes = json!([
        probe("rollback_world", rollback_world),
        probe("rollback_history", rollback_history),
        probe("atomic_scene_save", atomic_scene_save),
        probe("operation_jsonl_save", operation_jsonl_save)
    ]);
    write_events(
        CASE_ID,
        json!({"operations": parsed_log.apply_count()}),
        probes,
    );
    assert!(rollback_world && rollback_history && atomic_scene_save && operation_jsonl_save);
}

#[test]
fn t1_snapshot_whitelist() {
    const CASE_ID: &str = "t1_snapshot_whitelist";
    validate_case(
        CASE_ID,
        &["schema_version", "records"],
        &[
            "closed_whitelist_round_trip",
            "runtime_entity_bits_absent",
            "duplicate_component_unchanged",
            "delete_undo_restores_state",
        ],
    );

    let fixture = FixtureRoot::new("snapshot");
    let (mut world, entity) = named_world("Snapshot", true);
    let snapshot = capture_snapshot(&world, entity).unwrap();
    let snapshot_json = serde_json::to_string(&snapshot).unwrap();
    let decoded: EditorEntitySnapshot = serde_json::from_str(&snapshot_json).unwrap();
    let closed_whitelist_round_trip = decoded == snapshot
        && snapshot.records.len() == 4
        && snapshot.schema_version == EDITOR_SCHEMA_VERSION;
    let runtime_entity_bits_absent =
        !snapshot_json.contains("entity_bits") && !snapshot_json.contains("Entity");

    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();
    let before_duplicate = EditorHistory::state_sha256(&world).unwrap();
    let duplicate = history.apply(
        &mut EditorContext::new(
            &mut world,
            &lighting,
            "T1Snapshot",
            &fixture.0.join("scene.ron"),
            &fixture.0.join("operations.jsonl"),
        ),
        EditorOperation::AddComponent {
            entity_name: "Snapshot".into(),
            component: ComponentRecord::Transform {
                translation: [9.0, 9.0, 9.0],
                rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
        },
    );
    let duplicate_component_unchanged =
        matches!(duplicate, Err(EditorError::DuplicateComponent { .. }))
            && EditorHistory::state_sha256(&world).unwrap() == before_duplicate
            && history.operation_log().apply_count() == 0;

    let before_delete_undo = EditorHistory::state_sha256(&world).unwrap();
    let scene_path = fixture.0.join("scene.ron");
    let log_path = fixture.0.join("operations.jsonl");
    history
        .apply(
            &mut EditorContext::new(&mut world, &lighting, "T1Snapshot", &scene_path, &log_path),
            EditorOperation::Delete {
                entity_name: "Snapshot".into(),
            },
        )
        .unwrap();
    history
        .undo(&mut EditorContext::new(
            &mut world,
            &lighting,
            "T1Snapshot",
            &scene_path,
            &log_path,
        ))
        .unwrap();
    let delete_undo_restores_state =
        EditorHistory::state_sha256(&world).unwrap() == before_delete_undo;

    let probes = json!([
        probe("closed_whitelist_round_trip", closed_whitelist_round_trip),
        probe("runtime_entity_bits_absent", runtime_entity_bits_absent),
        probe(
            "duplicate_component_unchanged",
            duplicate_component_unchanged
        ),
        probe("delete_undo_restores_state", delete_undo_restores_state)
    ]);
    write_events(
        CASE_ID,
        json!({
            "schema_version": snapshot.schema_version,
            "records": snapshot.records.len()
        }),
        probes,
    );
    assert!(closed_whitelist_round_trip);
    assert!(runtime_entity_bits_absent);
    assert!(duplicate_component_unchanged);
    assert!(delete_undo_restores_state);
}

#[test]
fn t1_editor_command_replay() {
    const CASE_ID: &str = "t1_editor_command_replay";
    validate_case(
        CASE_ID,
        &["operations", "copy_rename_pairs"],
        &[
            "state_hash_replays",
            "all_ten_pairs_present",
            "generic_light_operation_replayed",
            "ui_event_to_operation",
            "queued_operation_persisted",
            "undo_updates_replay_log",
            "redo_updates_replay_log",
        ],
    );

    let fixture = FixtureRoot::new("replay");
    let scene_path = fixture.0.join("scene.ron");
    let log_path = fixture.0.join("operations.jsonl");
    let (mut world, _) = named_world("Source", false);
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();
    history
        .apply(
            &mut EditorContext::new(&mut world, &lighting, "T1Replay", &scene_path, &log_path),
            EditorOperation::AddComponent {
                entity_name: "Source".into(),
                component: ComponentRecord::Light {
                    config: LightConfig::default(),
                },
            },
        )
        .unwrap();
    for index in 1..=10 {
        let copy_name = format!("Source {index}");
        let copy = EditorOperation::Copy {
            source_name: "Source".into(),
            new_name: copy_name.clone(),
        };
        history
            .apply(
                &mut EditorContext::new(&mut world, &lighting, "T1Replay", &scene_path, &log_path),
                copy,
            )
            .unwrap();
        history
            .apply(
                &mut EditorContext::new(&mut world, &lighting, "T1Replay", &scene_path, &log_path),
                EditorOperation::Rename {
                    entity_name: copy_name,
                    new_name: format!("Edited {index}"),
                },
            )
            .unwrap();
    }

    let log = OperationLog::from_jsonl(&fs::read_to_string(&log_path).unwrap()).unwrap();
    let expected_hash = EditorHistory::state_sha256(&world).unwrap();
    let (mut replay_world, _) = named_world("Source", false);
    let mut replay_history = EditorHistory::default();
    replay_history
        .replay(
            &mut EditorContext::new(
                &mut replay_world,
                &lighting,
                "T1Replay",
                &scene_path,
                &log_path,
            ),
            &log,
        )
        .unwrap();
    let state_hash_replays = EditorHistory::state_sha256(&replay_world).unwrap() == expected_hash;
    let all_ten_pairs_present = (1..=10).all(|index| {
        replay_world
            .query::<&Name>()
            .iter()
            .any(|name| name.0 == format!("Edited {index}"))
    });
    let generic_light_operation_replayed = log
        .entries()
        .first()
        .is_some_and(|entry| matches!(&entry.operation, EditorOperation::AddComponent { .. }))
        && replay_world
            .query::<(&Name, &Light)>()
            .iter()
            .any(|(name, _)| name.0 == "Source");
    assert_eq!(log.apply_count(), 21);

    let probes = json!([
        probe("state_hash_replays", state_hash_replays),
        probe("all_ten_pairs_present", all_ten_pairs_present),
        probe(
            "generic_light_operation_replayed",
            generic_light_operation_replayed
        )
    ]);
    write_events(
        CASE_ID,
        json!({"operations": log.apply_count(), "copy_rename_pairs": 10}),
        probes,
    );
    assert!(state_hash_replays && all_ten_pairs_present && generic_light_operation_replayed);
}
