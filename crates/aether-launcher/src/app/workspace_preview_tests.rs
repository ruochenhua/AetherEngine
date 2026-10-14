use super::{cli::parse_args, ui, App, LauncherState};
use aether_engine::ecs::{
    components::{Camera, Selected, Transform},
    World,
};
use egui::{epaint::Shape, RawInput};
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
    ui::draw(
        app,
        1. / 60.,
        RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280., 960.),
            )),
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
