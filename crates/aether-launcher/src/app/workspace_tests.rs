//! Behavior tests for the shared Menu/Running workspace.

use super::{cli::parse_args, input, ui, workspace::WorkspaceTab, App, LauncherState};
use aether_engine::ecs::{
    components::{Camera, Selected, Transform},
    Entity, World,
};
use egui::{epaint::Shape, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};

fn app() -> App {
    App::new(parse_args(&["aether-launcher".into()]).unwrap())
}

fn running(app: &mut App) -> Entity {
    let mut world = World::new();
    let entity = world.spawn((Transform::default(), Camera::default(), Selected));
    app.state = LauncherState::Running {
        world,
        lighting: Default::default(),
    };
    app.show_overlay = true;
    entity
}

fn frame(app: &mut App, events: Vec<Event>) -> egui::FullOutput {
    ui::draw(
        app,
        1.0 / 60.0,
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0))),
            events,
            ..Default::default()
        },
    )
}

fn text_rect(output: &egui::FullOutput, label: &str) -> Option<Rect> {
    fn find(shape: &Shape, label: &str) -> Option<Rect> {
        match shape {
            Shape::Text(text) if text.galley.text() == label => {
                Some(Rect::from_min_size(text.pos, text.galley.size()))
            }
            Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, label)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|shape| find(&shape.shape, label))
}

fn click(app: &mut App, position: Pos2) -> egui::FullOutput {
    frame(
        app,
        vec![
            Event::PointerMoved(position),
            Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::default(),
            },
        ],
    );
    frame(
        app,
        vec![
            Event::PointerMoved(position),
            Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            },
        ],
    )
}

#[test]
fn ew0_workspace_tabs_survive_frames_and_scene_transition() {
    let mut app = app();
    app.workspace.select_tab(WorkspaceTab::Log);
    assert_eq!(app.workspace.tab, WorkspaceTab::Log);
    app.workspace.height = 212.0;
    frame(&mut app, vec![]);
    assert_eq!(app.workspace.tab, WorkspaceTab::Log);
    running(&mut app);
    frame(&mut app, vec![]);
    assert_eq!(app.workspace.tab, WorkspaceTab::Log);
    assert!((app.workspace.height - 212.0).abs() < 1.0);
    app.workspace.select_tab(WorkspaceTab::Project);
    frame(&mut app, vec![]);
    assert_eq!(app.workspace.tab, WorkspaceTab::Project);
}

#[test]
fn ew0_menu_error_and_tabs_are_accessible_in_menu_and_running() {
    let mut app = app();
    app.editor_operation_message = Some("Scene load failed: invalid RON".into());
    for is_running in [false, true] {
        if is_running {
            running(&mut app);
        }
        let output = frame(&mut app, vec![]);
        let log = text_rect(&output, "Log").expect("Log tab must be reachable");
        assert!(text_rect(&output, "Project").is_some());
        click(&mut app, log.center());
        let output = frame(&mut app, vec![]);
        assert_eq!(app.workspace.tab, WorkspaceTab::Log);
        assert!(text_rect(&output, "Scene load failed: invalid RON").is_some());
        let project = text_rect(&output, "Project").unwrap();
        click(&mut app, project.center());
        assert_eq!(app.workspace.tab, WorkspaceTab::Project);
    }
}

#[test]
fn ew0_resource_detail_preserves_scene_selection_and_inspector() {
    let mut app = app();
    let entity = running(&mut app);
    let path = std::path::PathBuf::from("assets/example.ron");
    app.workspace.select_resource(Some(path.clone()));
    assert_eq!(app.workspace.selected_resource.as_ref(), Some(&path));
    let output = frame(&mut app, vec![]);
    let LauncherState::Running { world, .. } = &app.state else {
        panic!("running")
    };
    assert!(world.query_one::<&Selected>(entity).get().is_ok());
    assert_eq!(crate::inspector::extract(world).unwrap().entity(), entity);
    click(
        &mut app,
        text_rect(&output, "Close resource details")
            .unwrap()
            .center(),
    );
    assert!(app.workspace.selected_resource.is_none());
    let LauncherState::Running { world, .. } = &app.state else {
        panic!("running")
    };
    assert_eq!(crate::inspector::extract(world).unwrap().entity(), entity);
    assert!(app.pending_select_entity.is_none());
}

#[test]
fn ew0_delete_in_focused_search_preserves_entity_and_scene_shortcut_works() {
    let mut app = app();
    let entity = running(&mut app);
    let output = frame(&mut app, vec![]);
    let search = text_rect(&output, "Search resources").expect("Project search field");
    click(&mut app, search.center());
    frame(
        &mut app,
        vec![Event::Key {
            key: egui::Key::Delete,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::default(),
        }],
    );
    let context = app.egui_ctx.clone();
    assert!(context.egui_wants_keyboard_input());
    input::process_scene_shortcuts(&mut app, true);
    assert!(
        app.pending_despawn_entity.is_none(),
        "Delete leaked from focused search"
    );
    context.memory_mut(|memory| {
        if let Some(id) = memory.focused() {
            memory.surrender_focus(id);
        }
    });
    assert!(!context.egui_wants_keyboard_input());
    input::process_scene_shortcuts(&mut app, true);
    assert_eq!(app.pending_despawn_entity, Some(entity));
}

#[test]
fn ew0_fullscreen_restores_workspace_tab_visibility_and_dimensions() {
    let mut app = app();
    running(&mut app);
    app.workspace.visible = true;
    app.workspace.tab = WorkspaceTab::Log;
    app.workspace.height = 228.0;
    frame(&mut app, vec![]);
    app.fullscreen_3d = true;
    let output = frame(&mut app, vec![]);
    assert!(text_rect(&output, "Log").is_none());
    assert!(text_rect(&output, "Scene").is_none());
    app.fullscreen_3d = false;
    let output = frame(&mut app, vec![]);
    assert!(
        text_rect(&output, "Log").is_some(),
        "Workspace did not restore"
    );
    assert!(text_rect(&output, "Scene").is_some());
    assert_eq!(app.workspace.tab, WorkspaceTab::Log);
    assert!(app.workspace.visible);
    assert!((app.workspace.height - 228.0).abs() < 1.0);
}

#[test]
fn ew0_no_gui_overlay_keeps_running_workspace_out_of_screenshot_mode() {
    let mut app = app();
    app.no_gui_overlay = true;
    assert!(text_rect(&frame(&mut app, vec![]), "Project").is_some());
    running(&mut app);
    let tab = app.workspace.tab;
    let height = app.workspace.height;
    let output = frame(&mut app, vec![]);
    assert!(
        output.shapes.is_empty(),
        "Screenshot mode included editor UI"
    );
    assert_eq!(app.workspace.tab, tab);
    assert_eq!(app.workspace.height, height);
}

#[test]
fn ew0_hidden_workspace_stays_hidden_through_fullscreen_and_reopens_from_menu() {
    let mut app = app();
    app.workspace.tab = WorkspaceTab::Log;
    app.workspace.height = 208.0;
    let output = frame(&mut app, vec![]);
    click(&mut app, text_rect(&output, "Hide").unwrap().center());
    assert!(!app.workspace.visible);
    running(&mut app);
    app.fullscreen_3d = true;
    frame(&mut app, vec![]);
    app.fullscreen_3d = false;
    let output = frame(&mut app, vec![]);
    assert!(!app.workspace.visible);
    assert!(text_rect(&output, "Hide").is_none());
    assert_eq!(app.workspace.tab, WorkspaceTab::Log);
    assert!((app.workspace.height - 208.0).abs() < 1.0);
    click(&mut app, text_rect(&output, "Log").unwrap().center());
    let output = frame(&mut app, vec![]);
    assert!(app.workspace.visible);
    assert!(text_rect(&output, "Hide").is_some());
    assert!((app.workspace.height - 208.0).abs() < 1.0);
}

#[test]
fn ew2_project_browser_shows_tree_query_controls_and_index_state_in_both_modes() {
    let mut app = app();
    app.workspace.set_project_root(project_root());
    for is_running in [false, true] {
        if is_running {
            running(&mut app);
        }
        let output = frame(&mut app, vec![]);
        for label in [
            "Scenes",
            "Assets",
            "Search resources",
            "All kinds",
            "Sort by",
            "Project resources",
        ] {
            assert!(
                text_rect(&output, label).is_some(),
                "Project browser should show {label:?} in menu/running mode"
            );
        }
    }
}

#[test]
fn ew2_selected_resource_details_include_type_size_and_validity_without_changing_entity() {
    let mut app = app();
    let root = valid_scene_project();
    app.workspace.set_project_root(root.clone());
    let entity = running(&mut app);
    let path = std::path::PathBuf::from("scenes/test.ron");
    app.workspace.select_resource(Some(path.clone()));
    let output = frame(&mut app, vec![]);
    assert!(text_rect(&output, "Type: Scene").is_some());
    assert!(text_rect(&output, "Status: Valid").is_some());
    assert!(text_starts_with(&output, "Size: "));
    let LauncherState::Running { world, .. } = &app.state else {
        panic!("running")
    };
    assert!(world.query_one::<&Selected>(entity).get().is_ok());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ew2_folder_tree_breadcrumb_and_history_navigate_deterministically() {
    let mut app = app();
    let root = valid_scene_project();
    std::fs::create_dir_all(root.join("assets/models")).unwrap();
    std::fs::write(root.join("assets/models/ship.obj"), "v 0 0 0\n").unwrap();
    app.workspace.set_project_root(root.clone());
    let output = frame(&mut app, vec![]);
    click(&mut app, text_rect(&output, "Scenes").unwrap().center());
    assert_eq!(app.workspace.project_directory(), "scenes");

    let output = frame(&mut app, vec![]);
    click(&mut app, text_rect(&output, "← Back").unwrap().center());
    assert_eq!(app.workspace.project_directory(), "");
    let output = frame(&mut app, vec![]);
    click(&mut app, text_rect(&output, "Forward →").unwrap().center());
    assert_eq!(app.workspace.project_directory(), "scenes");

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ew2_double_click_valid_scene_requests_transactional_open() {
    let mut app = app();
    let root = valid_scene_project();
    app.workspace.set_project_root(root.clone());
    let output = frame(&mut app, vec![]);
    let row = text_rect(&output, "test.ron  ·  Scene").expect("scene resource row");
    let position = row.center();
    frame(
        &mut app,
        vec![
            Event::PointerMoved(position),
            Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::default(),
            },
            Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            },
            Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::default(),
            },
            Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            },
        ],
    );
    assert_eq!(
        app.pending_project_open.as_ref(),
        Some(&root.join("scenes/test.ron"))
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ew2_search_shows_explicit_no_results_state() {
    let mut app = app();
    let root = valid_scene_project();
    app.workspace.set_project_root(root.clone());
    let output = frame(&mut app, vec![]);
    click(
        &mut app,
        text_rect(&output, "Search resources").unwrap().center(),
    );
    let output = frame(&mut app, vec![Event::Text("not-found".into())]);
    assert!(text_rect(&output, "No results").is_some());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ew2_refresh_exposes_scanning_state_before_publishing_snapshot() {
    let mut app = app();
    let root = valid_scene_project();
    app.workspace.set_project_root(root.clone());
    let output = frame(&mut app, vec![]);
    let output = click(&mut app, text_rect(&output, "Refresh").unwrap().center());
    assert!(text_rect(&output, "Scanning project…").is_some());
    std::thread::sleep(std::time::Duration::from_millis(20));
    let output = frame(&mut app, vec![]);
    assert!(text_rect(&output, "Project index ready").is_some());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ew2_invalid_scene_parse_preserves_selected_world_and_emits_queryable_error() {
    use tracing_subscriber::prelude::*;

    let (layer, receiver) = crate::logging::capture(4);
    let dispatch = tracing::Dispatch::new(tracing_subscriber::registry().with(layer));
    let root = valid_scene_project();
    let broken = root.join("scenes/broken.ron");
    std::fs::write(&broken, "(").unwrap();
    let mut world = World::new();
    let entity = world.spawn((Transform::default(), Camera::default(), Selected));
    let error = aether_engine::scene::loader::SceneLoader::from_file(&broken).unwrap_err();
    tracing::dispatcher::with_default(&dispatch, || {
        crate::app::scene::report_load_error(&broken, &error);
    });
    let event = receiver.try_recv().expect("scene error should reach EW1");
    assert_eq!(event.level, "ERROR");
    assert_eq!(event.message, "Open scene error");
    assert_eq!(event.fields["path"], broken.display().to_string());
    assert!(world.query_one::<&Selected>(entity).get().is_ok());
    std::fs::remove_dir_all(root).unwrap();
}

fn project_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn text_starts_with(output: &egui::FullOutput, prefix: &str) -> bool {
    fn find(shape: &Shape, prefix: &str) -> bool {
        match shape {
            Shape::Text(text) => text.galley.text().starts_with(prefix),
            Shape::Vec(shapes) => shapes.iter().any(|shape| find(shape, prefix)),
            _ => false,
        }
    }
    output.shapes.iter().any(|shape| find(&shape.shape, prefix))
}

fn valid_scene_project() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "aether_project_panel_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(root.join("scenes")).unwrap();
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::write(
        root.join("scenes/test.ron"),
        "(name: \"Test\", camera: (position: (0.0, 0.0, 0.0)))",
    )
    .unwrap();
    root
}
