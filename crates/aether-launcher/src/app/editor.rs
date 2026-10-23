//! Bridge replayable editor operations into the live launcher scene.

mod asset_drop;
mod asset_validation;
mod image_decode;
mod material_document;
mod material_drop;
pub(crate) mod replay;
mod resource_path;

pub(crate) fn apply_pending_asset_drop(app: &mut App) {
    asset_drop::apply_pending_drop(app);
    material_drop::apply_pending_drop(app);
}

use super::{document_state::SceneDocumentState, App, LauncherState};
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
    match capture_document_snapshot(app) {
        Ok(snapshot) => {
            app.document_state = SceneDocumentState::opened(path, snapshot);
        }
        Err(error) => {
            app.document_state.mark_unknown();
            tracing::error!(%error, "could not establish scene document checkpoint");
        }
    }
}

pub(crate) fn start_unsaved_scene(app: &mut App) {
    reset_history(app);
    app.cli.scene = None;
}

pub(crate) fn checkpoint_untitled_scene(app: &mut App) {
    match capture_document_snapshot(app) {
        Ok(snapshot) => app.document_state = SceneDocumentState::new_untitled(snapshot),
        Err(error) => {
            app.document_state.mark_unknown();
            tracing::error!(%error, "could not establish new scene checkpoint");
        }
    }
}

pub(crate) fn update_save_path(app: &mut App, path: PathBuf) -> bool {
    match capture_document_snapshot(app) {
        Ok(snapshot) => {
            app.document_state.checkpoint(path.clone(), snapshot);
            app.cli.scene = Some(path.to_string_lossy().into_owned());
            true
        }
        Err(error) => {
            app.document_state.mark_unknown();
            tracing::error!(%error, "could not establish saved scene checkpoint");
            false
        }
    }
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
    refresh_document_state(app);
}

pub(crate) fn refresh_document_state(app: &mut App) {
    match capture_document_snapshot(app) {
        Ok(snapshot) => app.document_state.refresh(snapshot),
        Err(error) => {
            app.document_state.mark_unknown();
            tracing::error!(%error, "could not determine active scene dirty state");
        }
    }
}

pub(crate) fn checkpoint_auto_saved_edit(app: &mut App) {
    match capture_document_snapshot(app) {
        Ok(snapshot) => {
            if let Some(path) = app.cli.scene.as_deref().map(PathBuf::from) {
                app.document_state.checkpoint(path, snapshot);
            } else {
                app.document_state = SceneDocumentState::new_untitled(snapshot);
            }
        }
        Err(error) => {
            app.document_state.mark_unknown();
            tracing::error!(%error, "could not establish persisted scene checkpoint");
        }
    }
}

pub(crate) fn serialize_active_scene(app: &mut App, scene_name: &str) -> anyhow::Result<String> {
    serialized_scene(app, scene_name)
}

fn capture_document_snapshot(app: &mut App) -> anyhow::Result<String> {
    serialized_scene(app, "AetherDocumentSnapshot")
}

fn serialized_scene(app: &mut App, scene_name: &str) -> anyhow::Result<String> {
    use aether_engine::scene::CameraConfig;

    let playing = app.particle_runtime.playback
        != super::particle_runtime::physics_runtime::PlaybackState::Stopped;
    let simulation_snapshot = if playing {
        app.particle_runtime.scene_snapshot.as_ref()
    } else {
        None
    };
    let super::LauncherState::Running { world, lighting } = &mut app.state else {
        anyhow::bail!("no active scene World");
    };
    let simulated_world = playing
        .then(|| super::particle_runtime::physics_runtime::SceneSimulationSnapshot::capture(world));
    if let Some(snapshot) = simulation_snapshot {
        snapshot.restore(world);
    }
    let mut description =
        aether_engine::scene::serializer::serialize_world(world, lighting, scene_name);
    description.camera = CameraConfig {
        position: app.camera.position.to_array(),
        yaw: app.camera.yaw,
        pitch: app.camera.pitch,
        speed: app.camera.speed,
        fov: app.camera.fov.to_degrees(),
        near: app.camera.near,
        far: app.camera.far,
    };
    if let Some(simulated) = simulated_world {
        simulated.restore(world);
    }
    aether_engine::scene::serializer::to_ron_string(&description)
}

pub(crate) fn apply_pending_operation(app: &mut App) {
    if let Some(reason) = app.editor_write_degraded.as_ref() {
        app.pending_editor_operation = None;
        app.editor_operation_message = Some(format!(
            "编辑器写入已停止：{reason}。请重新打开场景或先修复磁盘文件。"
        ));
        return;
    }
    if !app.document_state.can_transition() {
        app.pending_editor_operation = None;
        app.editor_operation_message = Some("当前场景状态未知或需要恢复，暂不能编辑。".into());
        return;
    }
    if app.particle_runtime.playback
        != super::particle_runtime::physics_runtime::PlaybackState::Stopped
    {
        app.pending_editor_operation = None;
        app.editor_operation_message = Some("请先停止模拟，再修改场景。".into());
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
            checkpoint_auto_saved_edit(app);
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
            refresh_document_state(app);
        }
        UndoActionKind::Operation => match apply_history_action(app, false) {
            Ok(()) => {
                app.redo_action_order.push(UndoActionKind::Operation);
                checkpoint_auto_saved_edit(app);
            }
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
            refresh_document_state(app);
        }
        UndoActionKind::Operation => match apply_history_action(app, true) {
            Ok(()) => {
                app.undo_action_order.push(UndoActionKind::Operation);
                checkpoint_auto_saved_edit(app);
            }
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
        app.document_state.mark_recovery_required();
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

pub(super) fn operation_log_path(scene_path: &Path) -> PathBuf {
    scene_path.with_extension("editor.jsonl")
}

#[cfg(test)]
mod tests;
