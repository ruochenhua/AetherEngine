mod document_state_tests;

use super::*;
use crate::app::cli::parse_args;
use aether_engine::ecs::components::{Light, Name, Transform};
use aether_engine::ecs::World;
use aether_engine::editor::{ComponentRecord, OperationLog};
use aether_engine::renderer::light::LightType;
use aether_engine::scene::LightConfig;
use serde_json::json;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(1);

#[test]
fn queued_ui_operation_persists_and_undo_redo_updates_replay_log() {
    let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "aether_launcher_t1_editor_{}_{}",
        std::process::id(),
        sequence
    ));
    fs::create_dir_all(&root).unwrap();
    let scene_path = root.join("editor.ron");
    let log_path = operation_log_path(&scene_path);
    let cli = parse_args(&["aether-launcher".into()]).unwrap();
    let mut app = App::new(cli);
    let mut world = World::new();
    let entity = world.spawn((Transform::default(), Name("Selected".into())));
    app.state = LauncherState::Running {
        world,
        lighting: aether_engine::renderer::light::LightingUniforms::default(),
    };
    app.cli.scene = Some(scene_path.to_string_lossy().into_owned());
    activate_scene(&mut app, scene_path.clone());
    app.pending_editor_operation = Some(EditorOperation::AddComponent {
        entity_name: "Selected".into(),
        component: ComponentRecord::Light {
            config: LightConfig {
                light_type: LightType::Point,
                ..LightConfig::default()
            },
        },
    });
    apply_pending_operation(&mut app);
    let applied = matches!(app.state, LauncherState::Running { ref world, .. }
        if world.query_one::<&Light>(entity).get().is_ok())
        && scene_path.is_file()
        && OperationLog::from_jsonl(&fs::read_to_string(&log_path).unwrap())
            .unwrap()
            .apply_count()
            == 1
        && app.document_state.status() == super::super::document_state::DocumentStatus::Clean;

    apply_undo(&mut app);
    let undone = matches!(app.state, LauncherState::Running { ref world, .. }
        if world.query_one::<&Light>(entity).get().is_err())
        && OperationLog::from_jsonl(&fs::read_to_string(&log_path).unwrap())
            .unwrap()
            .apply_count()
            == 0
        && app.document_state.status() == super::super::document_state::DocumentStatus::Clean;

    apply_redo(&mut app);
    let redone = matches!(app.state, LauncherState::Running { ref world, .. }
        if world.query_one::<&Light>(entity).get().is_ok())
        && OperationLog::from_jsonl(&fs::read_to_string(&log_path).unwrap())
            .unwrap()
            .apply_count()
            == 1
        && app.document_state.status() == super::super::document_state::DocumentStatus::Clean;
    assert!(applied && undone && redone);

    if let Some(path) = std::env::var_os("AETHER_T1_EDITOR_APPLY_EVENTS") {
        fs::write(
            path,
            serde_json::to_vec_pretty(&json!({
                "case_id": "t1_editor_command_replay",
                "kind": "Fixture",
                "probes": [
                    { "name": "queued_operation_persisted", "value": { "kind": "Bool", "value": applied } },
                    { "name": "undo_updates_replay_log", "value": { "kind": "Bool", "value": undone } },
                    { "name": "redo_updates_replay_log", "value": { "kind": "Bool", "value": redone } }
                ]
            }))
            .unwrap(),
        )
        .unwrap();
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn degraded_editor_rejects_followup_history_writes() {
    let cli = parse_args(&["aether-launcher".into()]).unwrap();
    let mut app = App::new(cli);
    let mut world = World::new();
    world.spawn((Transform::default(), Name("Original".into())));
    app.state = LauncherState::Running {
        world,
        lighting: aether_engine::renderer::light::LightingUniforms::default(),
    };
    let failure = EditorError::PersistenceRollbackFailed {
        original: "operation log write failed".into(),
        rollback: "scene restore failed".into(),
    };
    mark_degraded(&mut app, &failure);
    app.pending_editor_operation = Some(EditorOperation::Rename {
        entity_name: "Original".into(),
        new_name: "Changed".into(),
    });

    apply_pending_operation(&mut app);

    assert!(app.pending_editor_operation.is_none());
    assert!(app
        .editor_operation_message
        .as_deref()
        .unwrap()
        .contains("写入已停止"));
    let LauncherState::Running { world, .. } = &app.state else {
        panic!("running")
    };
    assert!(world
        .query::<&Name>()
        .iter()
        .any(|name| name.0 == "Original"));
    assert!(!world
        .query::<&Name>()
        .iter()
        .any(|name| name.0 == "Changed"));
}
