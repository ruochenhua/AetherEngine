use aether_engine::{
    asset::{AssetUsageSnapshot, GpuCacheInventoryState, GpuResourceUsageSnapshot},
    editor::{HistoryResourceUse, WorldResourceUse},
};
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SceneMutationInventory {
    pub project_root: PathBuf,
    pub document_status: super::document_state::DocumentStatus,
    pub world_resources: Vec<WorldResourceUse>,
    pub history_resources: Vec<HistoryResourceUse>,
    pub persisted_log: super::persisted_log_usage::PersistedLogResourceSnapshot,
    pub app_requests: Vec<super::request_usage::AppRequestResourceUse>,
    pub watcher_resources: Vec<super::watcher_usage::WatcherResourceUsage>,
    pub asset_store: Vec<AssetUsageSnapshot>,
    pub gpu_textures: Option<GpuResourceUsageSnapshot>,
    pub log_panel: crate::logging::LogPanelResourceSnapshot,
    pub project_work: super::workspace::AsyncWorkSnapshot,
    pub legacy_history_entries: usize,
    pub blockers: Vec<String>,
}

impl SceneMutationInventory {
    pub(crate) fn can_plan_single_resource_rename(&self) -> bool {
        self.blockers.is_empty()
    }

    pub(crate) fn render(ui: &mut egui::Ui, inventory: &Self) -> bool {
        ui.heading("Resource mutation readiness");
        ui.label(format!("Project: {}", inventory.project_root.display()));
        ui.label(format!("Scene document: {:?}", inventory.document_status));
        ui.label(format!(
            "Observed owners: World {}, history {}, persisted log {}, app requests {}, watchers {}, AssetStore {}, GPU texture cache {}, log links {}",
            inventory.world_resources.len(),
            inventory.history_resources.len(),
            inventory.persisted_log.resources.len(),
            inventory.app_requests.len(),
            inventory.watcher_resources.len(),
            inventory.asset_store.len(),
            inventory.gpu_textures.as_ref().map_or(0, |snapshot| snapshot.resources.len()),
            inventory.log_panel.buffered_event_resources.len(),
        ));
        ui.separator();
        if inventory.can_plan_single_resource_rename() {
            ui.label("Inventory is complete and quiescent; a Move/Rename contract may be planned.");
        } else {
            ui.colored_label(egui::Color32::YELLOW, "Move/Rename planning is blocked:");
            for blocker in &inventory.blockers {
                ui.label(format!("• {blocker}"));
            }
        }
        ui.small("This report is read-only. It does not move, rename, or delete files.");
        ui.button("Close").clicked()
    }
}

pub(crate) fn menu_button(ui: &mut egui::Ui) -> bool {
    ui.separator();
    let clicked = ui.button("Resource Mutation Readiness...").clicked();
    if clicked {
        ui.close();
    }
    clicked
}

pub(crate) fn render_window(
    context: &egui::Context,
    inventory: Option<&SceneMutationInventory>,
) -> bool {
    let Some(inventory) = inventory else {
        return false;
    };
    let mut open = true;
    let mut close_button = false;
    egui::Window::new("Resource Mutation Readiness")
        .open(&mut open)
        .resizable(true)
        .show(context, |ui| {
            close_button = SceneMutationInventory::render(ui, inventory);
        });
    close_button || !open
}

pub(crate) fn apply_ui_requests(
    app: &mut super::App,
    capture_requested: bool,
    close: bool,
    project_open: Option<std::path::PathBuf>,
    project_file: Option<super::workspace::ProjectFileAction>,
) {
    if project_open.is_some() {
        app.pending_project_open = project_open;
    }
    if project_file.is_some() {
        app.pending_project_file = project_file;
    }
    if capture_requested {
        app.scene_mutation_inventory = Some(capture(app));
    }
    if close {
        app.scene_mutation_inventory = None;
    }
}

pub(crate) fn capture(app: &super::App) -> SceneMutationInventory {
    let project_root = app.workspace.project_root().to_path_buf();
    let document_status = app.document_state.status();
    let (world_resources, active_world) = match &app.state {
        super::LauncherState::Running { world, .. } => (
            aether_engine::editor::capture_world_resource_usage(world, &project_root),
            true,
        ),
        super::LauncherState::Menu => (Vec::new(), false),
    };
    let history_resources = app.editor_history.resource_usage(&project_root);
    let persisted_log = super::persisted_log_usage::capture_persisted_log_resource_usage(
        app.cli.scene.as_deref().map(std::path::Path::new),
        &project_root,
    );
    let app_requests = super::request_usage::capture_app_request_resource_usage(app);
    let watcher_resources = super::watcher_usage::capture_watcher_resource_usage(
        &app.asset_hot_reload_watcher,
        &app.material_asset_watcher,
        &project_root,
    );
    let asset_store = app.asset_manager.resource_usage_snapshot();
    let gpu_textures = app
        .texture_cache
        .as_ref()
        .map(|cache| cache.resource_usage_snapshot());
    let log_panel = app.logs.resource_usage_snapshot();
    let project_work = app.workspace.project_async_work();
    let legacy_history_entries = app.undo_stack.len() + app.redo_stack.len();
    let mut blockers = Vec::new();

    if document_status != super::document_state::DocumentStatus::Clean {
        blockers.push(format!("active document is {document_status:?}"));
    }
    if app.editor_write_degraded.is_some() {
        blockers.push("editor persistence is degraded".into());
    }
    if !active_world {
        blockers.push("active World is unavailable".into());
    }
    if app.particle_runtime.playback
        != super::particle_runtime::physics_runtime::PlaybackState::Stopped
    {
        blockers.push("simulation is not stopped".into());
    }
    if app.particle_runtime.scene_snapshot.is_some() || app.particle_runtime.physics.is_some() {
        blockers.push("simulation runtime still owns a scene snapshot or physics world".into());
    }
    if app.pending_scene_transition.is_some() || app.pending_transition_choice.is_some() {
        blockers.push("a scene transition is pending".into());
    }
    if legacy_history_entries > 0 {
        blockers.push(format!(
            "legacy undo/redo contains {legacy_history_entries} entries without typed resource locators"
        ));
    }
    if matches!(
        &persisted_log.state,
        super::persisted_log_usage::PersistedLogState::Unavailable(_)
    ) {
        blockers.push("persisted editor log inventory is unavailable".into());
    }
    match &gpu_textures {
        None => blockers.push("GPU texture cache inventory is unavailable".into()),
        Some(snapshot) if snapshot.state == GpuCacheInventoryState::Unavailable => {
            blockers.push("GPU texture cache inventory is incomplete".into())
        }
        Some(snapshot) if snapshot.unidentified_legacy_texture_entries != Some(0) => {
            blockers.push("GPU texture cache contains unidentified legacy entries".into())
        }
        Some(_) => {}
    }
    blockers.push("GPU material-cache inventory is unavailable".into());
    if project_work.scan_in_flight {
        blockers.push("Project resource scan is still in flight".into());
    }
    if !project_work.preview_requests.is_empty() {
        blockers.push(format!(
            "{} Project preview request(s) still own source paths",
            project_work.preview_requests.len()
        ));
    }
    if project_work.reference_target_in_flight.is_some() {
        blockers.push("Project reference analysis is still in flight".into());
    }
    if app.animation_fixture.is_some() {
        blockers.push("animation fixture retains loaded source assets".into());
    }
    if log_panel.receiver_queue_unobserved {
        blockers.push("runtime log receiver may contain unobserved resource paths".into());
    }

    SceneMutationInventory {
        project_root,
        document_status,
        world_resources,
        history_resources,
        persisted_log,
        app_requests,
        watcher_resources,
        asset_store,
        gpu_textures,
        log_panel,
        project_work,
        legacy_history_entries,
        blockers,
    }
}

#[cfg(test)]
mod tests;
