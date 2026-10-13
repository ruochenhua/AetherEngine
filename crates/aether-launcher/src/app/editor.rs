//! Bridge replayable editor operations into the live launcher scene.

mod asset_drop;
mod image_decode;
mod material_document;
mod material_drop;
pub(crate) mod replay;
mod resource_path;

pub(crate) fn apply_pending_asset_drop(app: &mut App) {
    asset_drop::apply_pending_drop(app);
    material_drop::apply_pending_drop(app);
}

use super::{App, LauncherState};
use aether_engine::editor::{EditorContext, EditorError, EditorOperation};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UndoActionKind {
    Legacy,
    Operation,
}

pub(crate) fn activate_scene(app: &mut App, path: PathBuf) {
    reset_history(app);
    app.cli.scene = Some(path.to_string_lossy().into_owned());
}

pub(crate) fn start_unsaved_scene(app: &mut App) {
    reset_history(app);
    app.cli.scene = None;
}

pub(crate) fn update_save_path(app: &mut App, path: PathBuf) {
    app.cli.scene = Some(path.to_string_lossy().into_owned());
}

fn reset_history(app: &mut App) {
    app.editor_history = aether_engine::editor::EditorHistory::default();
    app.undo_stack.clear();
    app.redo_stack.clear();
    app.undo_action_order.clear();
    app.redo_action_order.clear();
    app.pending_editor_operation = None;
    app.pending_asset_drop = None;
    app.pending_material_drop = None;
    app.editor_operation_message = None;
    app.editor_write_degraded = None;
}

pub(crate) fn mark_legacy_edit(app: &mut App) {
    app.undo_action_order.push(UndoActionKind::Legacy);
    app.redo_action_order.clear();
}

pub(crate) fn apply_pending_operation(app: &mut App) {
    if let Some(reason) = app.editor_write_degraded.as_ref() {
        app.pending_editor_operation = None;
        app.editor_operation_message = Some(format!(
            "编辑器写入已停止：{reason}。请重新打开场景或先修复磁盘文件。"
        ));
        return;
    }
    if let Some(operation) = app.pending_editor_operation.take() {
        apply_operation(app, operation);
    }
}

fn apply_operation(app: &mut App, operation: EditorOperation) {
    let Some(scene_path) = app.cli.scene.as_deref().map(PathBuf::from) else {
        app.editor_operation_message = Some("先打开或保存一个场景，再执行可重放编辑操作。".into());
        return;
    };
    let log_path = operation_log_path(&scene_path);
    let scene_name = scene_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let result = {
        let App {
            state,
            editor_history,
            ..
        } = app;
        match state {
            LauncherState::Running { world, lighting } => Some(editor_history.apply(
                &mut EditorContext::new(world, lighting, &scene_name, &scene_path, &log_path),
                operation,
            )),
            LauncherState::Menu => None,
        }
    };
    match result {
        Some(Ok(())) => {
            app.undo_action_order.push(UndoActionKind::Operation);
            app.redo_action_order.clear();
            app.editor_operation_message = Some(format!(
                "已保存编辑操作；replay 日志：{}",
                log_path.display()
            ));
        }
        Some(Err(error)) => {
            mark_degraded(app, &error);
            app.editor_operation_message = Some(format!("编辑操作失败：{error}"));
            tracing::error!(%error, "editor operation failed");
        }
        None => app.editor_operation_message = Some("当前没有可编辑的场景。".into()),
    }
}

pub(crate) fn apply_undo(app: &mut App) {
    if let Some(reason) = app.editor_write_degraded.as_ref() {
        app.editor_operation_message = Some(format!("编辑器写入已停止：{reason}"));
        return;
    }
    let action = app
        .undo_action_order
        .pop()
        .unwrap_or(UndoActionKind::Legacy);
    match action {
        UndoActionKind::Legacy => {
            let Some(command) = app.undo_stack.pop() else {
                return;
            };
            if let LauncherState::Running { ref mut world, .. } = app.state {
                let redo = crate::inspector::apply_undo(world, &command);
                app.redo_stack.push(redo);
                app.redo_action_order.push(UndoActionKind::Legacy);
            } else {
                app.undo_stack.push(command);
                app.undo_action_order.push(UndoActionKind::Legacy);
            }
        }
        UndoActionKind::Operation => match apply_history_action(app, false) {
            Ok(()) => app.redo_action_order.push(UndoActionKind::Operation),
            Err(error) => {
                mark_degraded(app, &error);
                app.undo_action_order.push(UndoActionKind::Operation);
                app.editor_operation_message = Some(format!("撤销失败：{error}"));
                tracing::error!(%error, "editor operation undo failed");
            }
        },
    }
}

pub(crate) fn apply_redo(app: &mut App) {
    if let Some(reason) = app.editor_write_degraded.as_ref() {
        app.editor_operation_message = Some(format!("编辑器写入已停止：{reason}"));
        return;
    }
    let action = app
        .redo_action_order
        .pop()
        .unwrap_or(UndoActionKind::Legacy);
    match action {
        UndoActionKind::Legacy => {
            let Some(command) = app.redo_stack.pop() else {
                return;
            };
            if let LauncherState::Running { ref mut world, .. } = app.state {
                let undo = crate::inspector::apply_undo(world, &command);
                app.undo_stack.push(undo);
                app.undo_action_order.push(UndoActionKind::Legacy);
            } else {
                app.redo_stack.push(command);
                app.redo_action_order.push(UndoActionKind::Legacy);
            }
        }
        UndoActionKind::Operation => match apply_history_action(app, true) {
            Ok(()) => app.undo_action_order.push(UndoActionKind::Operation),
            Err(error) => {
                mark_degraded(app, &error);
                app.redo_action_order.push(UndoActionKind::Operation);
                app.editor_operation_message = Some(format!("重做失败：{error}"));
                tracing::error!(%error, "editor operation redo failed");
            }
        },
    }
}

pub(crate) fn mark_degraded(app: &mut App, error: &EditorError) {
    if matches!(error, EditorError::PersistenceRollbackFailed { .. }) {
        let reason = error.to_string();
        app.editor_write_degraded = Some(reason.clone());
        app.editor_operation_message = Some(format!(
            "编辑器进入只读保护：{reason}。请重新打开场景或恢复磁盘文件。"
        ));
    }
}

fn apply_history_action(app: &mut App, redo: bool) -> Result<(), EditorError> {
    let scene_path =
        app.cli
            .scene
            .as_deref()
            .map(PathBuf::from)
            .ok_or_else(|| EditorError::Io {
                path: "active scene".into(),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "no active scene path"),
            })?;
    let log_path = operation_log_path(&scene_path);
    let scene_name = scene_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let App {
        state,
        editor_history,
        ..
    } = app;
    let LauncherState::Running { world, lighting } = state else {
        return Err(EditorError::Serialization("no running scene".into()));
    };
    let mut context = EditorContext::new(world, lighting, &scene_name, &scene_path, &log_path);
    if redo {
        editor_history.redo(&mut context)
    } else {
        editor_history.undo(&mut context)
    }
}

fn operation_log_path(scene_path: &Path) -> PathBuf {
    scene_path.with_extension("editor.jsonl")
}

#[cfg(test)]
mod tests {
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
                == 1;

        apply_undo(&mut app);
        let undone = matches!(app.state, LauncherState::Running { ref world, .. }
            if world.query_one::<&Light>(entity).get().is_err())
            && OperationLog::from_jsonl(&fs::read_to_string(&log_path).unwrap())
                .unwrap()
                .apply_count()
                == 0;

        apply_redo(&mut app);
        let redone = matches!(app.state, LauncherState::Running { ref world, .. }
            if world.query_one::<&Light>(entity).get().is_ok())
            && OperationLog::from_jsonl(&fs::read_to_string(&log_path).unwrap())
                .unwrap()
                .apply_count()
                == 1;
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
}

#[cfg(test)]
mod material_drop_tests;

#[cfg(test)]
mod material_transaction_tests;

#[cfg(test)]
mod stale_texture_tests;

#[cfg(test)]
mod snapshot_budget_tests;

#[cfg(test)]
mod snapshot_source_cache_tests;
