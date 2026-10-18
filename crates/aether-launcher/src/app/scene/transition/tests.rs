use super::*;
use crate::app::{
    cli::parse_args,
    document_state::{DocumentIdentity, DocumentStatus},
    particle_runtime::physics_runtime::PlaybackState,
};
use aether_engine::{
    ecs::{
        components::{Name, Transform},
        World,
    },
    renderer::{camera::FlyCamera, light::LightingUniforms},
};
use egui::{epaint::Shape, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use glam::Vec3;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(1);

fn ui_frame(app: &mut App, events: Vec<Event>) -> egui::FullOutput {
    crate::app::ui::draw(
        app,
        1.0 / 60.0,
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0))),
            events,
            ..RawInput::default()
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

fn click(app: &mut App, position: Pos2) {
    ui_frame(
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
    ui_frame(
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
    );
}

fn dirty_app(scene_path: PathBuf) -> (App, aether_engine::ecs::Entity) {
    let mut app = App::new(parse_args(&["aether-launcher".into()]).unwrap());
    let mut world = World::new();
    let entity = world.spawn((Transform::default(), Name("Existing".into())));
    app.state = LauncherState::Running {
        world,
        lighting: LightingUniforms::default(),
    };
    app.camera = FlyCamera::default();
    crate::app::editor::activate_scene(&mut app, scene_path);
    app.camera.position += Vec3::X;
    crate::app::editor::refresh_document_state(&mut app);
    assert_eq!(app.document_state.status(), DocumentStatus::Dirty);
    (app, entity)
}

#[test]
fn cancel_keeps_the_dirty_document_world_and_identity() {
    let path = PathBuf::from("scenes/current.ron");
    let (mut app, entity) = dirty_app(path.clone());
    request_scene_transition(
        &mut app,
        SceneTransitionTarget::OpenPath(PathBuf::from("scenes/next.ron")),
    );
    app.pending_transition_choice = Some(SceneTransitionChoice::Cancel);

    process_scene_transition(&mut app);

    assert!(app.pending_scene_transition.is_none());
    assert_eq!(app.document_state.status(), DocumentStatus::Dirty);
    assert_eq!(
        app.document_state.identity(),
        &DocumentIdentity::Saved(path)
    );
    let LauncherState::Running { ref world, .. } = app.state else {
        panic!("running scene")
    };
    assert!(world.query_one::<&Name>(entity).get().is_ok());
}

#[test]
fn play_rejects_scene_transition_without_queueing_it() {
    let (mut app, _) = dirty_app(PathBuf::from("scenes/current.ron"));
    app.particle_runtime.playback = PlaybackState::Playing;

    request_scene_transition(&mut app, SceneTransitionTarget::CloseApplication);

    assert!(app.pending_scene_transition.is_none());
    assert!(app
        .editor_operation_message
        .as_deref()
        .unwrap()
        .contains("停止模拟"));
}

#[test]
fn second_transition_request_does_not_replace_the_first_target() {
    let (mut app, _) = dirty_app(PathBuf::from("scenes/current.ron"));
    let requested = PathBuf::from("scenes/first.ron");
    request_scene_transition(&mut app, SceneTransitionTarget::OpenPath(requested.clone()));
    request_scene_transition(&mut app, SceneTransitionTarget::NewScene);

    assert_eq!(
        app.pending_scene_transition.as_ref().unwrap().target,
        SceneTransitionTarget::OpenPath(requested)
    );
}

#[test]
fn save_failure_keeps_the_old_identity_and_dirty_state() {
    let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "aether_scene_transition_save_failure_{}_{}",
        std::process::id(),
        sequence
    ));
    std::fs::create_dir_all(&root).unwrap();
    let blocked_parent = root.join("not-a-directory");
    std::fs::write(&blocked_parent, "file").unwrap();
    let path = blocked_parent.join("current.ron");
    let (mut app, _) = dirty_app(path.clone());

    assert!(!save_scene(&mut app, false));

    assert_eq!(app.document_state.status(), DocumentStatus::Dirty);
    assert_eq!(
        app.document_state.identity(),
        &DocumentIdentity::Saved(path)
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn successful_save_advances_the_existing_document_checkpoint() {
    let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "aether_scene_transition_save_success_{}_{}",
        std::process::id(),
        sequence
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("current.ron");
    let (mut app, _) = dirty_app(path.clone());

    assert!(save_scene(&mut app, false));

    assert_eq!(app.document_state.status(), DocumentStatus::Clean);
    assert_eq!(
        app.document_state.identity(),
        &DocumentIdentity::Saved(path.clone())
    );
    assert!(path.is_file());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn dirty_transition_modal_cancel_button_keeps_document_open() {
    let (mut app, _) = dirty_app(PathBuf::from("scenes/current.ron"));
    assert!(!app.no_gui_overlay);
    request_scene_transition(&mut app, SceneTransitionTarget::NewScene);
    assert!(app
        .pending_scene_transition
        .as_ref()
        .is_some_and(|transition| transition.choice.is_none()));
    ui_frame(&mut app, vec![]);
    let output = ui_frame(&mut app, vec![]);
    let cancel = text_rect(&output, "Cancel").unwrap_or_else(|| {
        panic!(
            "modal Cancel button missing from {} shapes: {:?}",
            output.shapes.len(),
            output.shapes
        )
    });

    click(&mut app, cancel.center());

    assert_eq!(
        app.pending_transition_choice,
        Some(SceneTransitionChoice::Cancel)
    );
    process_scene_transition(&mut app);
    assert!(app.pending_scene_transition.is_none());
    assert_eq!(app.document_state.status(), DocumentStatus::Dirty);
}

#[test]
fn escape_cancels_dirty_transition_modal() {
    let (mut app, _) = dirty_app(PathBuf::from("scenes/current.ron"));
    request_scene_transition(&mut app, SceneTransitionTarget::CloseApplication);

    ui_frame(
        &mut app,
        vec![Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::default(),
        }],
    );

    assert_eq!(
        app.pending_transition_choice,
        Some(SceneTransitionChoice::Cancel)
    );
    process_scene_transition(&mut app);
    assert!(!app.exit_requested);
    assert_eq!(app.document_state.status(), DocumentStatus::Dirty);
}

#[test]
fn enter_activates_the_initially_focused_cancel_button() {
    let (mut app, _) = dirty_app(PathBuf::from("scenes/current.ron"));
    request_scene_transition(&mut app, SceneTransitionTarget::NewScene);
    ui_frame(&mut app, vec![]);
    ui_frame(&mut app, vec![]);

    ui_frame(
        &mut app,
        vec![Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::default(),
        }],
    );

    assert_eq!(
        app.pending_transition_choice,
        Some(SceneTransitionChoice::Cancel)
    );
}
