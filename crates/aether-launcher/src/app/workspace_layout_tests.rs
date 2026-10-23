//! Layout and resize behavior for the shared editor workspace.

use super::{cli::parse_args, ui, App, LauncherState};
use aether_engine::ecs::{
    components::{Camera, Selected, Transform},
    World,
};
use egui::{epaint::Shape, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};

fn app() -> App {
    App::new(parse_args(&["aether-launcher".into()]).unwrap())
}

fn running(app: &mut App) {
    let mut world = World::new();
    world.spawn((Transform::default(), Camera::default(), Selected));
    app.state = LauncherState::Running {
        world,
        lighting: Default::default(),
    };
    app.show_overlay = true;
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

fn click_at(app: &mut App, position: Pos2) -> egui::FullOutput {
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

fn text_count(output: &egui::FullOutput, label: &str) -> usize {
    fn count(shape: &Shape, label: &str) -> usize {
        match shape {
            Shape::Text(text) if text.galley.text() == label => 1,
            Shape::Vec(shapes) => shapes.iter().map(|shape| count(shape, label)).sum(),
            _ => 0,
        }
    }
    output
        .shapes
        .iter()
        .map(|shape| count(&shape.shape, label))
        .sum()
}

#[test]
fn ew3_running_workspace_has_one_tab_row_and_readable_default_height() {
    let mut app = app();
    running(&mut app);

    let output = frame(&mut app, vec![]);

    assert_eq!(text_count(&output, "Log"), 1);
    assert!(app.workspace.height >= 240.0);
}

#[test]
fn ew3_project_browser_hides_close_details_when_nothing_is_selected() {
    let mut app = app();
    app.workspace.set_project_root(project_root());
    let output = frame(&mut app, vec![]);

    assert!(text_rect(&output, "Close resource details").is_none());
}

#[test]
fn viewport_shows_mouse_navigation_tips_by_default() {
    let mut app = app();
    running(&mut app);

    let output = frame(&mut app, vec![]);

    assert_eq!(text_count(&output, "Viewport Controls"), 1);
    assert_eq!(text_count(&output, "LMB drag"), 1);
    assert_eq!(text_count(&output, "MMB drag"), 1);
    assert_eq!(text_count(&output, "LMB + WASD/QE"), 1);
    assert_eq!(text_count(&output, "Mac Trackpad"), 0);
}

#[test]
fn viewport_tips_can_be_collapsed_and_reopened() {
    let mut app = app();
    running(&mut app);
    let output = frame(&mut app, vec![]);
    let close_button = text_rect(&output, "×").unwrap().center();

    click_at(&mut app, close_button);
    let collapsed = frame(&mut app, vec![]);
    assert!(!app.viewport_tips.expanded);
    assert_eq!(text_count(&collapsed, "Viewport Controls"), 0);
    let reopen = text_rect(&collapsed, "? Viewport").unwrap().center();

    click_at(&mut app, reopen);
    let expanded = frame(&mut app, vec![]);

    assert!(app.viewport_tips.expanded);
    assert_eq!(text_count(&expanded, "Viewport Controls"), 1);
}

#[test]
fn ew3_project_folder_panel_can_be_resized_by_dragging_its_edge() {
    let mut app = app();
    app.workspace.set_project_root(project_root());
    running(&mut app);
    frame(&mut app, vec![]);

    let panel_id = egui::Id::new("project_folder_panel");
    let panel_state = egui::containers::panel::PanelState::load(&app.egui_ctx, panel_id)
        .expect("folder column should use egui's resizable Panel state");
    let resize_id = panel_id.with("__resize");
    let handle = app
        .egui_ctx
        .read_response(resize_id)
        .expect("egui Panel should register its native resize handle");
    let initial_width = panel_state.size().x;
    let start = handle.rect.center();
    let end = start + Vec2::new(90.0, 0.0);
    frame(
        &mut app,
        vec![
            Event::PointerMoved(start),
            Event::PointerButton {
                pos: start,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::default(),
            },
        ],
    );
    // Model pointer input over many frames. A single large synthetic jump
    // would hide code that mistakes per-frame drag delta for total delta.
    for step in 1..=9 {
        frame(
            &mut app,
            vec![Event::PointerMoved(
                start + Vec2::new(step as f32 * 10.0, 0.0),
            )],
        );
        let response = app
            .egui_ctx
            .read_response(resize_id)
            .expect("egui Panel resize handle should remain registered while dragging");
        assert!(
            response.dragged(),
            "egui Panel resize handle did not capture drag step {step}"
        );
    }
    frame(
        &mut app,
        vec![Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::default(),
        }],
    );

    frame(&mut app, vec![]);
    let resized_width = egui::containers::panel::PanelState::load(&app.egui_ctx, panel_id)
        .expect("resized folder panel state should remain available")
        .size()
        .x;
    assert!(
        resized_width >= initial_width + 40.0,
        "folder width changed from {initial_width} to {resized_width} after dragging"
    );
}

fn project_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
