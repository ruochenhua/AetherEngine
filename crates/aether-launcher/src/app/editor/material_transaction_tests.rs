use super::{
    apply_pending_asset_drop, apply_redo, apply_undo, material_drop, material_drop_tests::Fixture,
    replay::replay_resolved_log,
};
use crate::{
    app::{cli::parse_args, workspace::MaterialDropTarget, App, LauncherState},
    project_index::ResourceKind,
};
use aether_engine::{
    asset::{registry::BuiltinMeshRegistry, AssetManager},
    ecs::{
        components::{MaterialAssetRef, MaterialAssetStatus, Selected},
        World,
    },
    editor::{EditorContext, EditorHistory, OperationLog},
    renderer::{
        extracted_material::ExtractedMaterial, renderable::MaterialUniform,
        transparent::TransparentMaterial,
    },
    scene::{loader::SceneLoader, material::TextureUsage, MaterialConfig},
};
use std::fs;

#[test]
fn host_queued_drop_persists_undo_redo_and_replay_runtime_exactly() {
    let mut f = Fixture::new();
    let initial = f
        .world
        .query_one::<&MaterialConfig>(f.entity)
        .get()
        .unwrap()
        .clone();
    let mut app = App::new(parse_args(&["aether-launcher".into()]).unwrap());
    app.workspace.set_project_root(f.root.clone());
    app.asset_manager = std::mem::replace(&mut f.assets, AssetManager::with_project_root(&f.root));
    app.state = LauncherState::Running {
        world: std::mem::replace(&mut f.world, World::new()),
        lighting: Default::default(),
    };
    let scene = f.root.join("scenes/host.ron");
    let log = scene.with_extension("editor.jsonl");
    app.cli.scene = Some(scene.to_string_lossy().into_owned());
    app.pending_material_drop = Some(f.request(MaterialDropTarget::Asset));
    apply_pending_asset_drop(&mut app);
    assert_eq!(app.editor_history.operation_log().apply_count(), 1);
    assert_eq!(app.undo_action_order.len(), 1);
    let LauncherState::Running { world, .. } = &app.state else {
        panic!("world")
    };
    let expected = world
        .query_one::<&MaterialUniform>(f.entity)
        .get()
        .unwrap()
        .to_owned();
    let extracted = world
        .query_one::<&ExtractedMaterial>(f.entity)
        .get()
        .unwrap()
        .clone();
    assert!(world.query_one::<&Selected>(f.entity).get().is_ok());
    let bytes = (fs::read(&scene).unwrap(), fs::read(&log).unwrap());
    app.pending_material_drop = Some(f.request(MaterialDropTarget::Texture(TextureUsage::Albedo)));
    apply_pending_asset_drop(&mut app);
    assert!(app
        .editor_operation_message
        .as_deref()
        .unwrap()
        .contains("read-only"));
    assert_eq!((fs::read(&scene).unwrap(), fs::read(&log).unwrap()), bytes);
    assert_eq!(app.editor_history.operation_log().apply_count(), 1);
    assert_eq!(app.undo_action_order.len(), 1);
    apply_undo(&mut app);
    let LauncherState::Running { world, .. } = &app.state else {
        panic!("world")
    };
    assert_eq!(
        *world.query_one::<&MaterialConfig>(f.entity).get().unwrap(),
        initial
    );
    apply_redo(&mut app);
    let LauncherState::Running { world, lighting } = &mut app.state else {
        panic!("world")
    };
    assert_eq!(
        *world.query_one::<&MaterialUniform>(f.entity).get().unwrap(),
        expected
    );
    assert_eq!(
        *world
            .query_one::<&ExtractedMaterial>(f.entity)
            .get()
            .unwrap(),
        extracted
    );
    let replay_log = OperationLog::from_jsonl(&fs::read_to_string(&log).unwrap()).unwrap();
    let mut replay_history = EditorHistory::default();
    let replay_scene = f.root.join("scenes/replay.ron");
    let replay_file = replay_scene.with_extension("editor.jsonl");
    replay_resolved_log(
        &mut replay_history,
        &mut EditorContext::new(world, lighting, "replay", &replay_scene, &replay_file),
        &replay_log,
        &f.root,
        &f.device,
        &mut app.asset_manager,
    )
    .unwrap();
    assert_eq!(
        *world.query_one::<&MaterialUniform>(f.entity).get().unwrap(),
        expected
    );
    assert_eq!(
        *world
            .query_one::<&ExtractedMaterial>(f.entity)
            .get()
            .unwrap(),
        extracted
    );
    assert!(world
        .query_one::<&MaterialAssetStatus>(f.entity)
        .get()
        .unwrap()
        .0
        .is_none());
    let mut reopened = World::new();
    SceneLoader::open_scene(
        &scene,
        &f.device,
        &BuiltinMeshRegistry::new(),
        &mut app.asset_manager,
        &mut reopened,
    )
    .unwrap();
    assert_eq!(
        reopened
            .query::<&MaterialAssetRef>()
            .iter()
            .next()
            .unwrap()
            .0,
        "assets/materials/sample.ron"
    );
}

#[test]
fn inline_transparent_drop_undo_redo_replay_and_scene_reopen_agree() {
    let mut f = Fixture::new();
    fs::copy(
        f.root.join("assets/materials/color.png"),
        f.root.join("assets/materials/old.png"),
    )
    .unwrap();
    let material = MaterialConfig {
        transparent: Some(aether_engine::scene::TransparentMaterialConfig {
            texture: Some("assets/materials/old.png".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    f.world
        .insert(f.entity, (material.clone(), TransparentMaterial::default()))
        .unwrap();
    let initial_uniform = *f
        .world
        .query_one::<&MaterialUniform>(f.entity)
        .get()
        .unwrap();
    let request = f.request(MaterialDropTarget::Texture(TextureUsage::Albedo));
    let (operation, prepared) = f.prepare(&request).unwrap();
    let expected_config = prepared.material.clone();
    let expected_uniform = prepared.uniform;
    let expected_transparent = prepared.transparent_material.clone().unwrap();
    let scene = f.root.join("scenes/inline.ron");
    let log = scene.with_extension("editor.jsonl");
    let lighting = Default::default();
    let mut history = EditorHistory::default();
    let mut ctx = EditorContext::new(&mut f.world, &lighting, "inline", &scene, &log);
    history
        .apply_prepared_material(&mut ctx, operation, prepared)
        .unwrap();
    history.undo(&mut ctx).unwrap();
    assert_eq!(
        *ctx.world
            .query_one::<&MaterialConfig>(f.entity)
            .get()
            .unwrap(),
        material
    );
    assert_eq!(
        *ctx.world
            .query_one::<&MaterialUniform>(f.entity)
            .get()
            .unwrap(),
        initial_uniform
    );
    history.redo(&mut ctx).unwrap();
    assert_eq!(
        *ctx.world
            .query_one::<&TransparentMaterial>(f.entity)
            .get()
            .unwrap(),
        expected_transparent
    );
    let parsed_log = OperationLog::from_jsonl(&fs::read_to_string(&log).unwrap()).unwrap();
    let mut replay_history = EditorHistory::default();
    replay_resolved_log(
        &mut replay_history,
        &mut ctx,
        &parsed_log,
        &f.root,
        &f.device,
        &mut f.assets,
    )
    .unwrap();
    assert_eq!(
        *ctx.world
            .query_one::<&MaterialConfig>(f.entity)
            .get()
            .unwrap(),
        expected_config
    );
    assert_eq!(
        *ctx.world
            .query_one::<&MaterialUniform>(f.entity)
            .get()
            .unwrap(),
        expected_uniform
    );
    assert!(ctx.world.query_one::<&Selected>(f.entity).get().is_ok());
    let mut reopened = World::new();
    SceneLoader::open_scene(
        &scene,
        &f.device,
        &BuiltinMeshRegistry::new(),
        &mut f.assets,
        &mut reopened,
    )
    .unwrap();
    assert_eq!(
        *reopened.query::<&MaterialConfig>().iter().next().unwrap(),
        expected_config
    );
    let reopened_transparent = reopened
        .query::<&TransparentMaterial>()
        .iter()
        .next()
        .unwrap()
        .clone();
    assert_eq!(
        reopened_transparent.base_color,
        expected_transparent.base_color
    );
    assert_eq!(reopened_transparent.blend, expected_transparent.blend);
    assert_eq!(
        reopened_transparent.alpha_cutoff,
        expected_transparent.alpha_cutoff
    );
    let texture = f.assets.get(reopened_transparent.texture.unwrap()).unwrap();
    assert_eq!(
        *texture,
        aether_engine::asset::texture::CpuTexture::from_file(
            &f.root.join("assets/materials/color.png")
        )
        .unwrap()
    );
}

#[test]
fn host_failed_validation_and_persistence_keep_files_selection_and_history() {
    let mut f = Fixture::new();
    let initial = f
        .world
        .query_one::<&MaterialConfig>(f.entity)
        .get()
        .unwrap()
        .clone();
    let scene = f.root.join("scenes/failure.ron");
    let log = scene.with_extension("editor.jsonl");
    fs::write(&scene, "baseline").unwrap();
    fs::write(&log, "baseline log").unwrap();
    let mut app = App::new(parse_args(&["aether-launcher".into()]).unwrap());
    app.workspace.set_project_root(f.root.clone());
    app.asset_manager = std::mem::replace(&mut f.assets, AssetManager::with_project_root(&f.root));
    app.state = LauncherState::Running {
        world: std::mem::replace(&mut f.world, World::new()),
        lighting: Default::default(),
    };
    app.cli.scene = Some(scene.to_string_lossy().into_owned());
    let mut request = f.request(MaterialDropTarget::Texture(TextureUsage::Normal));
    request.resource.kind = ResourceKind::Material;
    app.pending_material_drop = Some(request);
    material_drop::apply_pending_drop(&mut app);
    assert_eq!(fs::read_to_string(&scene).unwrap(), "baseline");
    assert_eq!(fs::read_to_string(&log).unwrap(), "baseline log");
    // A directory in place of the log file forces persistence failure before committing history.
    fs::remove_file(&log).unwrap();
    fs::create_dir(&log).unwrap();
    app.pending_material_drop = Some(f.request(MaterialDropTarget::Texture(TextureUsage::Normal)));
    material_drop::apply_pending_drop(&mut app);
    assert_eq!(fs::read_to_string(&scene).unwrap(), "baseline");
    assert!(log.is_dir());
    assert_eq!(app.editor_history.operation_log().apply_count(), 0);
    assert!(app.undo_action_order.is_empty() && app.redo_action_order.is_empty());
    let LauncherState::Running { world, .. } = &app.state else {
        panic!("world")
    };
    assert_eq!(
        *world.query_one::<&MaterialConfig>(f.entity).get().unwrap(),
        initial
    );
    assert!(world.query_one::<&Selected>(f.entity).get().is_ok());
}

#[test]
fn document_relative_parent_texture_reference_stays_within_assets() {
    let mut f = Fixture::new();
    fs::create_dir_all(f.root.join("assets/textures")).unwrap();
    fs::copy(
        f.root.join("assets/materials/color.png"),
        f.root.join("assets/textures/color.png"),
    )
    .unwrap();
    let config = MaterialConfig {
        albedo_texture: Some("../textures/color.png".into()),
        ..Default::default()
    };
    fs::write(
        f.root.join("assets/materials/sample.ron"),
        ron::ser::to_string(&config).unwrap(),
    )
    .unwrap();
    let request = f.request(MaterialDropTarget::Asset);
    let (_, prepared) = f.prepare(&request).unwrap();
    assert_eq!(prepared.material, config);
    assert!(prepared.extracted_material.albedo_texture().is_some());
}
