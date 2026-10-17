use super::*;
use crate::project_dependencies::{DependencyReport, Resolution};
use aether_engine::ecs::{components::Selected, World};
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

pub(super) struct Fixture(PathBuf);
impl Fixture {
    pub(super) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "aether-reference-panel-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("assets")).unwrap();
        fs::create_dir_all(root.join("scenes")).unwrap();
        fs::write(root.join("assets/t.png"), b"endpoint").unwrap();
        Self(root)
    }
    pub(super) fn write(&self, path: &str, content: &str) {
        fs::write(self.0.join(path), content).unwrap();
    }
    pub(super) fn panel(&self, target: &str) -> ProjectPanel {
        let mut panel = ProjectPanel::new(self.0.clone());
        panel.wait_for_refresh();
        panel.select_resource(Some(PathBuf::from(target)));
        panel
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
pub(super) fn finish(panel: &mut ProjectPanel) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while panel.references.busy() {
        panel.references.poll();
        assert!(Instant::now() < deadline, "analysis did not finish");
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn report(panel: &ProjectPanel) -> &DependencyReport {
    panel.references.report().unwrap()
}

#[test]
fn references_pin_target_and_back_restores_project_navigation_without_reanalysis() {
    let f = Fixture::new();
    f.write("scenes/a.ron", r#"(name:"A",camera:(position:(0.0,0.0,0.0)),objects:[(name:"one",mesh:File("assets/t.png")),(name:"two",mesh:File("assets/t.png"))])"#);
    let mut panel = f.panel("assets/t.png");
    panel.show_references();
    finish(&mut panel);
    assert_eq!(
        report(&panel).used_by().count(),
        2,
        "duplicates must survive UI wiring"
    );
    let fingerprint = report(&panel).fingerprint;
    let edge = report(&panel).used_by().next().unwrap().clone();
    let mut world = World::new();
    let selected = world.spawn((Selected,));
    panel.select_reference_source(&edge);
    assert_eq!(panel.selected_resource(), Some("scenes/a.ron"));
    assert_eq!(panel.current_directory(), "scenes");
    assert_eq!(panel.references.target(), Some("assets/t.png"));
    assert!(world.query_one::<&Selected>(selected).get().is_ok());
    panel.back_to_reference_target();
    assert_eq!(panel.selected_resource(), Some("assets/t.png"));
    assert_eq!(panel.current_directory(), "assets");
    assert!(!panel.references.busy());
    assert_eq!(report(&panel).fingerprint, fingerprint);
}

#[test]
fn references_publish_loading_complete_empty_incomplete_and_failed_states() {
    let f = Fixture::new();
    let mut panel = f.panel("assets/t.png");
    panel.show_references();
    assert_eq!(panel.references.status(), "Loading");
    finish(&mut panel);
    assert_eq!(panel.references.status(), "Empty");
    f.write("assets/m.ron", r#"(normal_texture:Some("t.png"))"#);
    panel.show_references();
    finish(&mut panel);
    assert_eq!(panel.references.status(), "Complete");
    f.write("scenes/bad.ron", "(");
    panel.show_references();
    finish(&mut panel);
    assert_eq!(panel.references.status(), "Incomplete");
    assert!(!report(&panel).diagnostics.is_empty());
    fs::remove_dir_all(&f.0).unwrap();
    panel.show_references();
    finish(&mut panel);
    assert_eq!(panel.references.status(), "Failed");
    assert!(panel.references.error().is_some());
}

#[test]
fn references_explicit_async_recheck_detects_content_and_inventory_changes() {
    let f = Fixture::new();
    let mut panel = f.panel("assets/t.png");
    panel.show_references();
    finish(&mut panel);
    panel.references.recheck();
    assert!(panel.references.busy());
    finish(&mut panel);
    assert_eq!(panel.references.status(), "Empty");
    fs::write(f.0.join("assets/t.png"), b"changed!").unwrap();
    panel.references.recheck();
    assert!(panel.references.busy());
    finish(&mut panel);
    assert_eq!(panel.references.status(), "Stale");
    panel.references.reanalyze();
    finish(&mut panel);
    assert_eq!(panel.references.status(), "Empty");
    fs::create_dir(f.0.join("assets/added")).unwrap();
    panel.references.recheck();
    finish(&mut panel);
    assert_eq!(panel.references.status(), "Stale");
}

#[test]
fn references_missing_unsafe_and_non_file_rows_cannot_navigate() {
    let f = Fixture::new();
    f.write(
        "assets/m.ron",
        r#"(normal_texture:Some("missing.png"),orm_texture:Some("../../escape.png"))"#,
    );
    let mut panel = f.panel("assets/m.ron");
    panel.show_references();
    finish(&mut panel);
    let edges = report(&panel).edges.clone();
    assert!(edges
        .iter()
        .any(|e| matches!(e.resolution, Resolution::Missing)));
    assert!(edges
        .iter()
        .any(|e| matches!(e.resolution, Resolution::Unsafe(_))));
    panel.select_resource(Some(PathBuf::from("assets/t.png")));
    for edge in edges {
        panel.select_reference_source(&edge);
    }
    assert_eq!(panel.selected_resource(), Some("assets/t.png"));
    assert_eq!(panel.references.target(), Some("assets/m.ron"));
    let mut non_file = report(&panel).edges[0].clone();
    non_file.resolution = Resolution::Resolved;
    non_file.source = "https://example.invalid/source".into();
    panel.select_reference_source(&non_file);
    assert_eq!(panel.selected_resource(), Some("assets/t.png"));
}

#[test]
fn references_close_and_focus_scoped_escape_preserve_project_runtime_and_files() {
    use tracing_subscriber::prelude::*;
    let f = Fixture::new();
    f.write("assets/m.ron", "(roughness:0.5)");
    let mut app =
        crate::app::App::new(crate::app::cli::parse_args(&["aether-launcher".into()]).unwrap());
    app.workspace.project = f.panel("assets/t.png");
    app.asset_manager = aether_engine::asset::AssetManager::with_project_root(f.0.clone());
    let material = app
        .asset_manager
        .load::<aether_engine::asset::material_asset::MaterialAsset>("assets/m.ron")
        .unwrap();
    let asset = app.asset_manager.get(material).unwrap();
    let mut world = World::new();
    let selected = world.spawn((Selected,));
    app.state = crate::app::LauncherState::Running {
        world,
        lighting: Default::default(),
    };
    let history = (app.editor_history.undo_len(), app.editor_history.redo_len());
    let panel = &mut app.workspace.project;
    let generation = panel.index.snapshot().generation;
    let bytes = fs::read(f.0.join("assets/t.png")).unwrap();
    let (layer, receiver) = crate::logging::capture(8);
    let dispatch = tracing::Dispatch::new(tracing_subscriber::registry().with(layer));
    tracing::dispatcher::with_default(&dispatch, || {
        panel.show_references();
        finish(panel);
        assert!(!panel.references.escape(false, false));
        assert!(!panel.references.escape(true, true));
        assert_eq!(panel.references.target(), Some("assets/t.png"));
        assert!(panel.references.escape(true, false));
        assert_eq!(panel.references.target(), None);
        panel.show_references();
        panel.references.close();
        std::thread::sleep(Duration::from_millis(10));
        panel.references.poll();
        assert_eq!(
            panel.references.target(),
            None,
            "late result must not reopen report"
        );
    });
    assert_eq!(panel.selected_resource(), Some("assets/t.png"));
    assert_eq!(panel.current_directory(), "assets");
    assert_eq!(panel.index.snapshot().generation, generation);
    assert_eq!(fs::read(f.0.join("assets/t.png")).unwrap(), bytes);
    let crate::app::LauncherState::Running { world, .. } = &app.state else {
        panic!("scene changed")
    };
    assert_eq!(world.len(), 1);
    assert!(world.query_one::<&Selected>(selected).get().is_ok());
    assert!(std::sync::Arc::ptr_eq(
        &asset,
        &app.asset_manager.get(material).unwrap()
    ));
    assert_eq!(
        (app.editor_history.undo_len(), app.editor_history.redo_len()),
        history
    );
    assert!(app.pending_select_entity.is_none());
    assert!(app.pending_project_open.is_none());
    assert!(receiver.try_recv().is_err());
}
