use super::{cli::parse_args, ui, App, LauncherState};
use aether_engine::ecs::{
    components::{Camera, Selected, Transform},
    World,
};
use egui::{epaint::Shape, Event, Modifiers, PointerButton, RawInput, Vec2};
use std::path::PathBuf;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "aether-preview-ui-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("assets")).unwrap();
        image::RgbaImage::new(400, 100)
            .save(path.join("assets/image.png"))
            .unwrap();
        image::RgbaImage::new(384, 384)
            .save(path.join("assets/square.png"))
            .unwrap();
        std::fs::write(
            path.join("assets/model.obj"),
            "v -4 0 0\nv 2 0 0\nv -4 3 1\nf 1 2 3\n",
        )
        .unwrap();
        std::fs::write(path.join("assets/broken.png"), "malformed image").unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn frame(app: &mut App) -> egui::FullOutput {
    frame_with_events(app, vec![])
}

fn frame_with_events(app: &mut App, events: Vec<Event>) -> egui::FullOutput {
    ui::draw(
        app,
        1. / 60.,
        RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280., 960.),
            )),
            events,
            ..Default::default()
        },
    )
}
fn contains(output: &egui::FullOutput, prefix: &str) -> bool {
    fn matches(shape: &Shape, prefix: &str) -> bool {
        match shape {
            Shape::Text(text) => text.galley.text().starts_with(prefix),
            Shape::Vec(shapes) => shapes.iter().any(|shape| matches(shape, prefix)),
            _ => false,
        }
    }
    output
        .shapes
        .iter()
        .any(|shape| matches(&shape.shape, prefix))
}

fn text_position(output: &egui::FullOutput, prefix: &str) -> Option<egui::Pos2> {
    fn find(shape: &Shape, prefix: &str) -> Option<egui::Pos2> {
        match shape {
            Shape::Text(text) if text.galley.text().starts_with(prefix) => Some(text.pos),
            Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, prefix)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|shape| find(&shape.shape, prefix))
}

fn textured_rects(output: &egui::FullOutput) -> Vec<(egui::TextureId, egui::Rect)> {
    fn collect(shape: &Shape, rects: &mut Vec<(egui::TextureId, egui::Rect)>) {
        match shape {
            Shape::Rect(rect) => {
                if let Some(brush) = &rect.brush {
                    rects.push((brush.fill_texture_id, rect.rect));
                }
            }
            Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, rects)),
            _ => {}
        }
    }
    let mut rects = Vec::new();
    output
        .shapes
        .iter()
        .for_each(|shape| collect(&shape.shape, &mut rects));
    rects
}

#[test]
fn ew3_selected_thumbnail_placeholder_transitions_without_selection_changes() {
    let fixture = Fixture::new();
    let mut app = App::new(parse_args(&["aether-launcher".into()]).unwrap());
    app.workspace.set_project_root(fixture.0.clone());
    app.workspace.height = 460.;
    let mut world = World::new();
    let entity = world.spawn((Transform::default(), Camera::default(), Selected));
    app.state = LauncherState::Running {
        world,
        lighting: Default::default(),
    };
    app.show_overlay = true;
    for (resource, ready) in [
        ("assets/image.png", "Image: 400 × 100 px"),
        ("assets/model.obj", "AABB: 6.000 × 3.000 × 1.000"),
        ("assets/broken.png", "Preview unavailable: Malformed image"),
    ] {
        let resource = fixture.0.join(resource);
        app.workspace.select_resource(Some(resource.clone()));
        let output = frame(&mut app);
        assert!(
            contains(&output, "Loading preview…"),
            "placeholder required before worker completion"
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let output = frame(&mut app);
            if contains(&output, ready) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "missing ready state: {ready}"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(
            app.workspace.selected_resource.as_deref(),
            Some(resource.strip_prefix(&fixture.0).unwrap())
        );
        let LauncherState::Running { world, .. } = &app.state else {
            panic!("running")
        };
        assert!(world.query_one::<&Selected>(entity).get().is_ok());
        assert!(app.pending_select_entity.is_none());
        assert!(app.pending_project_open.is_none());
    }
}

#[test]
fn ew3_selected_preview_uses_the_open_space_to_the_right_of_the_resource_list() {
    let fixture = Fixture::new();
    let mut app = App::new(parse_args(&["aether-launcher".into()]).unwrap());
    app.workspace.set_project_root(fixture.0.clone());
    app.workspace.height = 460.;
    app.workspace
        .select_resource(Some(fixture.0.join("assets/image.png")));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let output = loop {
        let output = frame(&mut app);
        if contains(&output, "Image: 400 × 100 px") {
            break output;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "image preview did not load"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    let resources_x = text_position(&output, "Resources").unwrap().x;
    let dimensions_x = text_position(&output, "Image: 400 × 100 px").unwrap().x;
    assert!(
        dimensions_x > resources_x + 120.,
        "preview metadata should be to the right of the resource list"
    );
}

#[test]
fn ew3_selected_preview_workspace_does_not_grow_after_being_resized_smaller() {
    let fixture = Fixture::new();
    let mut app = App::new(parse_args(&["aether-launcher".into()]).unwrap());
    app.workspace.set_project_root(fixture.0.clone());
    app.workspace.height = 460.;
    app.workspace
        .select_resource(Some(fixture.0.join("assets/square.png")));
    frame(&mut app);

    let id = egui::Id::new("editor_workspace");
    let resize_id = id.with("__resize");
    let initial = egui::containers::panel::PanelState::load(&app.egui_ctx, id)
        .expect("workspace panel state")
        .size()
        .y;
    let handle = app
        .egui_ctx
        .read_response(resize_id)
        .expect("workspace resize handle");
    let start = handle.rect.center();
    let end = start + Vec2::new(0., 100.);
    frame_with_events(
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
    for step in 1..=10 {
        let pos = start + Vec2::new(0., step as f32 * 10.);
        frame_with_events(&mut app, vec![Event::PointerMoved(pos)]);
    }
    frame_with_events(
        &mut app,
        vec![Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::default(),
        }],
    );

    let resized = egui::containers::panel::PanelState::load(&app.egui_ctx, id)
        .expect("resized workspace panel state")
        .size()
        .y;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let first_output = loop {
        let output = frame(&mut app);
        if contains(&output, "Image: 384 × 384 px") {
            break output;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "image preview did not load"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    let preview_rect = |output: &egui::FullOutput| {
        let textured_rects = textured_rects(output);
        assert_eq!(
            textured_rects.len(),
            1,
            "expected only the preview image texture"
        );
        textured_rects[0].1
    };
    let first_preview_rect = preview_rect(&first_output);
    let resources_x = text_position(&first_output, "Resources")
        .expect("resource list heading")
        .x;
    assert!(
        first_preview_rect.left() > resources_x + 120.,
        "preview image should remain in the details pane to the right of Resources"
    );
    for _ in 0..20 {
        let settled_output = frame(&mut app);
        let settled_preview_rect = preview_rect(&settled_output);
        assert!(
            (settled_preview_rect.height() - first_preview_rect.height()).abs() <= 1.,
            "preview image grew from {:.1}px to {:.1}px after loading",
            first_preview_rect.height(),
            settled_preview_rect.height()
        );
    }
    let settled = egui::containers::panel::PanelState::load(&app.egui_ctx, id)
        .expect("settled workspace panel state")
        .size()
        .y;
    assert!(resized < initial - 40.);
    assert!(
        settled <= resized + 1.,
        "workspace grew from {resized} to {settled}"
    );
}
