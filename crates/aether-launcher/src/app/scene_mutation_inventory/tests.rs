use super::*;
use crate::app::{cli::parse_args, editor::activate_scene, App, LauncherState};
use aether_engine::{
    asset::AssetKind,
    ecs::{
        components::{Name, Transform},
        World,
    },
    renderer::light::LightingUniforms,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::SystemTime,
};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(1);

fn fixture_root() -> PathBuf {
    let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "aether_mutation_inventory_{}_{}",
        std::process::id(),
        sequence
    ));
    std::fs::create_dir_all(root.join("scenes")).unwrap();
    root
}

#[test]
fn unavailable_lifecycle_and_runtime_sources_block_rename_planning() {
    let app = App::new(parse_args(&["aether-launcher".into()]).unwrap());

    let report = capture(&app);

    assert!(!report.can_plan_single_resource_rename());
    assert!(report.blockers.iter().any(|item| item.contains("Unknown")));
    assert!(report
        .blockers
        .iter()
        .any(|item| item.contains("World is unavailable")));
    assert!(report
        .blockers
        .iter()
        .any(|item| item.contains("GPU texture cache inventory is unavailable")));
    assert!(report
        .blockers
        .iter()
        .any(|item| item.contains("GPU material-cache inventory is unavailable")));
    assert!(report
        .blockers
        .iter()
        .any(|item| item.contains("Project resource scan is still in flight")));
}

#[test]
fn inventory_surfaces_live_owners_deterministically_without_consuming_them() {
    let root = fixture_root();
    let scene_path = root.join("scenes/current.ron");
    std::fs::write(
        &scene_path,
        r#"SceneDescription(name: "Current", camera: (position: (0.0, 0.0, 0.0)))"#,
    )
    .unwrap();
    let mut app = App::new(parse_args(&["aether-launcher".into()]).unwrap());
    app.workspace.set_project_root(root.clone());
    let mut world = World::new();
    world.spawn((Transform::default(), Name("Baseline".into())));
    app.state = LauncherState::Running {
        world,
        lighting: LightingUniforms::default(),
    };
    activate_scene(&mut app, scene_path);
    app.pending_project_open = Some(root.join("scenes/next.ron"));
    app.asset_hot_reload_watcher
        .prefab_modified
        .insert("assets/model.prefab".into(), SystemTime::UNIX_EPOCH);

    let first = capture(&app);
    let second = capture(&app);

    assert_eq!(first, second);
    assert_eq!(
        first.document_status,
        super::super::document_state::DocumentStatus::Clean
    );
    assert_eq!(
        first.persisted_log.state,
        super::super::persisted_log_usage::PersistedLogState::Missing
    );
    assert_eq!(first.app_requests.len(), 1);
    assert_eq!(
        first.app_requests[0].source.owner,
        super::super::request_usage::AppRequestOwner::ProjectOpen
    );
    assert_eq!(first.watcher_resources.len(), 1);
    assert_eq!(first.watcher_resources[0].kind, AssetKind::Prefab);
    assert!(first.project_work.preview_requests.is_empty());
    assert!(first.project_work.reference_target_in_flight.is_none());
    assert_eq!(first.legacy_history_entries, 0);
    assert!(!first.can_plan_single_resource_rename());
    assert_eq!(app.pending_project_open, Some(root.join("scenes/next.ron")));
    assert_eq!(app.asset_hot_reload_watcher.prefab_modified.len(), 1);

    let _ = std::fs::remove_dir_all(root);
}
