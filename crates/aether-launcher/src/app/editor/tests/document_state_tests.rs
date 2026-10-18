use super::super::{activate_scene, refresh_document_state, serialize_active_scene};
use crate::app::{
    cli::parse_args,
    document_state::DocumentStatus,
    particle_runtime::physics_runtime::{PlaybackState, SceneSimulationSnapshot},
    App, LauncherState,
};
use aether_engine::{
    ecs::{
        components::{Camera, Name, Selected, Transform},
        World,
    },
    renderer::{camera::FlyCamera, light::LightingUniforms},
};
use glam::Vec3;
use std::path::PathBuf;

fn app_with_scene() -> (App, aether_engine::ecs::Entity) {
    let mut app = App::new(parse_args(&["aether-launcher".into()]).unwrap());
    let mut world = World::new();
    let world_camera = Transform {
        translation: Vec3::new(3.0, 3.0, 3.0),
        ..Transform::default()
    };
    world.spawn((world_camera, Camera::default()));
    let entity = world.spawn((Transform::default(), Name("Object".into()), Selected));
    app.state = LauncherState::Running {
        world,
        lighting: LightingUniforms::default(),
    };
    app.camera = FlyCamera::default();
    activate_scene(&mut app, PathBuf::from("scenes/document-test.ron"));
    (app, entity)
}

#[test]
fn selection_does_not_dirty_but_viewport_camera_movement_does() {
    let (mut app, entity) = app_with_scene();
    let saved_camera_position = app.camera.position;
    assert_eq!(app.document_state.status(), DocumentStatus::Clean);

    let LauncherState::Running { ref mut world, .. } = app.state else {
        panic!("running scene")
    };
    world.remove::<(Selected,)>(entity).unwrap();
    refresh_document_state(&mut app);
    assert_eq!(app.document_state.status(), DocumentStatus::Clean);

    app.camera.position += Vec3::new(1.0, 0.0, 0.0);
    refresh_document_state(&mut app);
    assert_eq!(app.document_state.status(), DocumentStatus::Dirty);

    app.camera.position = saved_camera_position;
    refresh_document_state(&mut app);
    assert_eq!(app.document_state.status(), DocumentStatus::Clean);
}

#[test]
fn play_snapshot_excludes_simulated_transforms_without_changing_live_world() {
    let (mut app, entity) = app_with_scene();
    let LauncherState::Running { ref world, .. } = app.state else {
        panic!("running scene")
    };
    app.particle_runtime.scene_snapshot = Some(SceneSimulationSnapshot::capture(world));
    app.particle_runtime.playback = PlaybackState::Playing;

    let LauncherState::Running { ref mut world, .. } = app.state else {
        panic!("running scene")
    };
    world
        .query_one_mut::<&mut Transform>(entity)
        .unwrap()
        .translation = Vec3::new(8.0, 2.0, -3.0);

    refresh_document_state(&mut app);

    assert_eq!(app.document_state.status(), DocumentStatus::Clean);
    let LauncherState::Running { ref world, .. } = app.state else {
        panic!("running scene")
    };
    let live_position = world
        .query_one::<&Transform>(entity)
        .get()
        .unwrap()
        .translation;
    assert_eq!(live_position, Vec3::new(8.0, 2.0, -3.0));
}

#[test]
fn save_serialization_stages_viewport_camera_without_mutating_world() {
    let (mut app, _) = app_with_scene();
    app.camera.position = Vec3::new(10.0, 2.0, 3.0);

    let serialized = serialize_active_scene(&mut app, "CameraStage").unwrap();
    let description = aether_engine::scene::SceneDescription::from_ron(&serialized).unwrap();
    assert_eq!(description.camera.position, [10.0, 2.0, 3.0]);

    let LauncherState::Running { ref world, .. } = app.state else {
        panic!("running scene")
    };
    let world_camera_position = world
        .query::<(&Transform, &Camera)>()
        .iter()
        .next()
        .unwrap()
        .0
        .translation;
    assert_eq!(world_camera_position, Vec3::new(3.0, 3.0, 3.0));
}
