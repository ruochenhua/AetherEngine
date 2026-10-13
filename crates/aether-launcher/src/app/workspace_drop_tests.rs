use super::{cli::parse_args, ui, App, LauncherState};
use aether_engine::ecs::{
    components::{Name, Selected, Transform},
    World,
};
use egui::{epaint::Shape, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use std::{fs, path::PathBuf};

fn app_with_obj() -> (App, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("aether-ew3-ui-{}-{id}", std::process::id()));
    fs::create_dir_all(root.join("assets/models")).unwrap();
    fs::write(root.join("assets/models/sample.obj"), "v 0 0 0\n").unwrap();
    let mut app = App::new(parse_args(&["aether-launcher".into()]).unwrap());
    app.workspace.set_project_root(root.clone());
    let mut world = World::new();
    world.spawn((Transform::default(), Name("Selected".into()), Selected));
    app.state = LauncherState::Running {
        world,
        lighting: Default::default(),
    };
    app.show_overlay = true;
    (app, root)
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

fn drag(app: &mut App, source: Pos2, target: Pos2) {
    frame(
        app,
        vec![
            Event::PointerMoved(source),
            Event::PointerButton {
                pos: source,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::default(),
            },
        ],
    );
    frame(
        app,
        vec![Event::PointerMoved(source + Vec2::new(12.0, 0.0))],
    );
    frame(app, vec![Event::PointerMoved(target)]);
    frame(
        app,
        vec![Event::PointerButton {
            pos: target,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::default(),
        }],
    );
}

#[test]
fn supported_obj_drop_is_routed_only_from_project_to_viewport() {
    let (mut app, root) = app_with_obj();
    let output = frame(&mut app, vec![]);
    let source = text_rect(&output, "sample.obj  ·  OBJ")
        .expect("OBJ project row")
        .center();
    drag(&mut app, source, Pos2::new(640.0, 300.0));

    let request = app
        .pending_asset_drop
        .as_ref()
        .expect("viewport drop request");
    assert_eq!(request.relative_path, "assets/models/sample.obj");
    let selected_before = match &app.state {
        LauncherState::Running { world, .. } => world.query::<&Selected>().iter().count(),
        LauncherState::Menu => 0,
    };
    assert_eq!(
        selected_before, 1,
        "dragging must not change scene selection"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn obj_drag_cancelled_or_released_outside_viewport_is_a_noop() {
    let (mut app, root) = app_with_obj();
    let output = frame(&mut app, vec![]);
    let source = text_rect(&output, "sample.obj  ·  OBJ")
        .expect("OBJ project row")
        .center();
    drag(&mut app, source, Pos2::new(640.0, 12.0));

    assert!(app.pending_asset_drop.is_none());
    let LauncherState::Running { world, .. } = &app.state else {
        panic!("running scene must remain active")
    };
    assert_eq!(world.query::<&Selected>().iter().count(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unsupported_gltf_resource_cannot_start_a_placement_drop() {
    let (mut app, root) = app_with_obj();
    fs::write(root.join("assets/models/unsupported.gltf"), "{}").unwrap();
    app.workspace.set_project_root(root.clone());
    let output = frame(&mut app, vec![]);
    let source = text_rect(&output, "unsupported.gltf  ·  glTF")
        .expect("glTF project row")
        .center();
    drag(&mut app, source, Pos2::new(640.0, 300.0));

    assert!(app.pending_asset_drop.is_none());
    let LauncherState::Running { world, .. } = &app.state else {
        panic!("running scene must remain active")
    };
    assert_eq!(world.query::<&Selected>().iter().count(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn material_and_image_project_rows_never_become_viewport_placement() {
    let (mut app, root) = app_with_obj();
    fs::write(
        root.join("assets/models/material.ron"),
        ron::ser::to_string(&aether_engine::scene::MaterialConfig {
            albedo_texture: Some("image.png".into()),
            ..Default::default()
        })
        .unwrap(),
    )
    .unwrap();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([255; 4]))
        .save(root.join("assets/models/image.png"))
        .unwrap();
    app.workspace.set_project_root(root.clone());
    for label in ["material.ron  ·  Material", "image.png  ·  Image"] {
        let output = frame(&mut app, vec![]);
        let source = text_rect(&output, label)
            .expect("project resource row")
            .center();
        frame(
            &mut app,
            vec![
                Event::PointerMoved(source),
                Event::PointerButton {
                    pos: source,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::default(),
                },
            ],
        );
        frame(
            &mut app,
            vec![Event::PointerMoved(source + Vec2::new(12.0, 0.0))],
        );
        let payload =
            egui::DragAndDrop::payload::<super::workspace::AssetDragPayload>(&app.egui_ctx)
                .expect("supported resource must begin typed drag");
        assert_eq!(
            payload.kind,
            if label.contains("Material") {
                crate::project_index::ResourceKind::Material
            } else {
                crate::project_index::ResourceKind::Image
            }
        );
        let destination = Pos2::new(640.0, 300.0);
        frame(&mut app, vec![Event::PointerMoved(destination)]);
        frame(
            &mut app,
            vec![Event::PointerButton {
                pos: destination,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            }],
        );
        assert!(app.pending_asset_drop.is_none());
    }
    fs::remove_dir_all(root).unwrap();
}
