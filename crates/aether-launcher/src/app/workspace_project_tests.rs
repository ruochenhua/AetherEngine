use super::{cli::parse_args, scene, App, LauncherState};
use aether_engine::ecs::{
    components::{Camera, Name, Transform},
    World,
};
use std::path::PathBuf;

fn app() -> App {
    App::new(parse_args(&["aether-launcher".into()]).unwrap())
}

fn fixture_root() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "aether-ew2-review-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(path.join("scenes")).unwrap();
    path
}

fn running(app: &mut App) -> aether_engine::ecs::Entity {
    let mut world = World::new();
    let sentinel = world.spawn((
        Transform::default(),
        Camera::default(),
        Name("OldWorld".into()),
    ));
    app.state = LauncherState::Running {
        world,
        lighting: Default::default(),
    };
    sentinel
}

fn headless_device() -> wgpu::Device {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("headless adapter required for project scene open");
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .expect("headless device required for project scene open")
        .0
}

#[test]
fn project_open_requested_in_menu_is_retained_until_running() {
    let mut app = app();
    let root = fixture_root();
    let path = root.join("scenes/next.ron");
    app.pending_project_open = Some(path.clone());

    scene::process_post_ui_ops(&mut app);

    assert!(matches!(app.state, LauncherState::Menu));
    assert_eq!(app.pending_project_open.as_ref(), Some(&path));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn selecting_log_resource_reveals_its_parent_directory() {
    let mut app = app();
    let root = fixture_root();
    std::fs::create_dir_all(root.join("assets/models/ship")).unwrap();
    let resource = root.join("assets/models/ship/hull.obj");
    std::fs::write(&resource, "o hull\n").unwrap();
    app.workspace.set_project_root(root.clone());
    app.workspace.select_resource(Some(resource));

    assert_eq!(app.workspace.project_directory(), "assets/models/ship");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn browser_open_processing_replaces_the_running_scene() {
    use tracing_subscriber::prelude::*;

    let (layer, receiver) = crate::logging::capture(4);
    let dispatch = tracing::Dispatch::new(tracing_subscriber::registry().with(layer));
    let mut app = app();
    let root = fixture_root();
    let path = root.join("scenes/loaded.ron");
    std::fs::write(
        &path,
        r#"SceneDescription(name: "LoadedByBrowser", camera: (position: (0.0, 0.0, 0.0)))"#,
    )
    .unwrap();
    running(&mut app);
    app.pending_project_open = Some(path);

    let device = headless_device();
    tracing::dispatcher::with_default(&dispatch, || {
        scene::process_project_browser_open_with_device(&mut app, &device)
    });

    let LauncherState::Running { world, .. } = &app.state else {
        panic!("running")
    };
    let names = world
        .query::<&Name>()
        .iter()
        .map(|name| name.0.clone())
        .collect::<Vec<_>>();
    assert!(
        !names.iter().any(|name| name == "OldWorld"),
        "old scene content must be removed"
    );
    assert!(
        names.iter().any(|name| name == "Camera"),
        "new camera must be loaded"
    );
    assert!(
        names.iter().any(|name| name == "DirectionalLight"),
        "default light must be loaded"
    );
    assert_eq!(
        world.len(),
        2,
        "minimal scene should contain camera and light"
    );
    let events = std::iter::from_fn(|| receiver.try_recv().ok()).collect::<Vec<_>>();
    assert!(events
        .iter()
        .any(|event| event.message == "Opened scene from project browser"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn browser_open_failure_preserves_world_and_emits_structured_error() {
    use tracing_subscriber::prelude::*;

    let (layer, receiver) = crate::logging::capture(4);
    let dispatch = tracing::Dispatch::new(tracing_subscriber::registry().with(layer));
    let mut app = app();
    let root = fixture_root();
    let path = root.join("scenes/broken.ron");
    std::fs::write(&path, "(").unwrap();
    let old_entity = running(&mut app);
    app.pending_project_open = Some(path.clone());

    let device = headless_device();
    tracing::dispatcher::with_default(&dispatch, || {
        scene::process_project_browser_open_with_device(&mut app, &device)
    });

    let LauncherState::Running { world, .. } = &app.state else {
        panic!("running")
    };
    assert!(
        world.contains(old_entity),
        "failed browser open must preserve the current world"
    );
    let event = receiver
        .try_recv()
        .expect("failed scene open should emit an EW1 error");
    assert_eq!(event.level, "ERROR");
    assert_eq!(event.message, "Open scene error");
    assert_eq!(event.fields["path"], path.display().to_string());
    std::fs::remove_dir_all(root).unwrap();
}
