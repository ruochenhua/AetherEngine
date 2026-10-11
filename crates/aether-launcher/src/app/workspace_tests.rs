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
