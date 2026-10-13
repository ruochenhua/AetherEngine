use super::{material_drop::prepare_material_drop, material_drop_tests::Fixture};
use crate::app::workspace::MaterialDropTarget;
use aether_engine::{
    editor::{EditorContext, EditorHistory},
    scene::material::TextureUsage,
};

fn changed_image_is_fresh(target: MaterialDropTarget) {
    let mut f = Fixture::new();
    let path = f.root.join("assets/materials/color.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]))
        .save(&path)
        .unwrap();
    let request = f.request(target);
    let (operation, prepared) = f.prepare(&request).unwrap();
    let red = prepared.extracted_material.albedo_texture().unwrap();
    assert_eq!(&f.assets.get(red).unwrap().data[..4], &[255, 0, 0, 255]);
    let scene = f.root.join("scenes/fresh.ron");
    let log = scene.with_extension("editor.jsonl");
    let lighting = Default::default();
    EditorHistory::default()
        .apply_prepared_material(
            &mut EditorContext::new(&mut f.world, &lighting, "fresh", &scene, &log),
            operation,
            prepared,
        )
        .unwrap();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 255, 255]))
        .save(&path)
        .unwrap();
    let (_, second) = prepare_material_drop(&request, &f.root, &mut f.assets, &f.world).unwrap();
    let blue = second.extracted_material.albedo_texture().unwrap();
    assert_eq!(
        &f.assets.get(blue).unwrap().data[..4],
        &[0, 0, 255, 255],
        "a validated replacement file must replace cached red pixels"
    );
    assert_ne!(blue, red);
    assert_ne!(
        f.assets.asset_id(blue).unwrap(),
        f.assets.asset_id(red).unwrap()
    );
    assert_eq!(&f.assets.get(red).unwrap().data[..4], &[255, 0, 0, 255]);
}

#[test]
fn inline_drop_refreshes_pixels_after_same_path_external_rewrite() {
    changed_image_is_fresh(MaterialDropTarget::Texture(TextureUsage::Albedo));
}

#[test]
fn linked_material_drop_refreshes_document_relative_texture_dependencies() {
    changed_image_is_fresh(MaterialDropTarget::Asset);
}

#[test]
fn rejected_material_config_does_not_invalidate_the_last_good_texture() {
    let mut f = Fixture::new();
    let request = f.request(MaterialDropTarget::Asset);
    let (_, first) = f.prepare(&request).unwrap();
    let original = first.extracted_material.albedo_texture().unwrap();
    let original_pixels = f.assets.get(original).unwrap().data.clone();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 255, 255]))
        .save(f.root.join("assets/materials/color.png"))
        .unwrap();
    let invalid = aether_engine::scene::MaterialConfig {
        albedo_texture: Some("color.png".into()),
        normal_scale: 4.0,
        ..Default::default()
    };
    std::fs::write(
        f.root.join("assets/materials/sample.ron"),
        ron::ser::to_string(&invalid).unwrap(),
    )
    .unwrap();
    assert!(f.prepare(&request).is_err());
    assert_eq!(
        f.assets
            .get(original)
            .expect("rejected config must retain its previous texture generation")
            .data,
        original_pixels
    );
}

#[test]
fn invalid_later_dependency_does_not_refresh_an_earlier_valid_image() {
    let mut f = Fixture::new();
    let request = f.request(MaterialDropTarget::Asset);
    let (_, first) = f.prepare(&request).unwrap();
    let original = first.extracted_material.albedo_texture().unwrap();
    let original_pixels = f.assets.get(original).unwrap().data.clone();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 255, 255]))
        .save(f.root.join("assets/materials/color.png"))
        .unwrap();
    let invalid = aether_engine::scene::MaterialConfig {
        albedo_texture: Some("color.png".into()),
        normal_texture: Some("missing.png".into()),
        ..Default::default()
    };
    std::fs::write(
        f.root.join("assets/materials/sample.ron"),
        ron::ser::to_string(&invalid).unwrap(),
    )
    .unwrap();
    assert!(f.prepare(&request).is_err());
    assert_eq!(
        f.assets
            .get(original)
            .expect("validate every dependency before refreshing any")
            .data,
        original_pixels
    );
}

fn shared_red_world(
    f: &mut Fixture,
    history: &mut EditorHistory,
    scene: &std::path::Path,
    log: &std::path::Path,
) -> aether_engine::ecs::Entity {
    use aether_engine::{
        ecs::components::{MeshHandle, Name, Transform, Visibility},
        renderer::{extracted_material::ExtractedMaterial, renderable::MaterialUniform},
        scene::MaterialConfig,
    };
    let request = f.request(MaterialDropTarget::Texture(TextureUsage::Albedo));
    image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]))
        .save(f.root.join("assets/materials/color.png"))
        .unwrap();
    let (operation, prepared) = f.prepare(&request).unwrap();
    let lighting = Default::default();
    history
        .apply_prepared_material(
            &mut EditorContext::new(&mut f.world, &lighting, "shared", scene, log),
            operation,
            prepared,
        )
        .unwrap();
    let mesh = f
        .world
        .query_one::<&MeshHandle>(f.entity)
        .get()
        .unwrap()
        .clone();
    let material = f
        .world
        .query_one::<&MaterialConfig>(f.entity)
        .get()
        .unwrap()
        .clone();
    let uniform = *f
        .world
        .query_one::<&MaterialUniform>(f.entity)
        .get()
        .unwrap();
    let extracted = f
        .world
        .query_one::<&ExtractedMaterial>(f.entity)
        .get()
        .unwrap()
        .clone();
    f.world.spawn((
        Name("Other Mesh".into()),
        Transform::default(),
        Visibility::default(),
        mesh,
        material,
        uniform,
        extracted,
    ))
}

fn assert_pixels(f: &Fixture, entity: aether_engine::ecs::Entity, expected: [u8; 4]) {
    let handle = f
        .world
        .query_one::<&aether_engine::renderer::extracted_material::ExtractedMaterial>(entity)
        .get()
        .unwrap()
        .albedo_texture()
        .unwrap();
    assert_eq!(
        &f.assets
            .get(handle)
            .expect("every current consumer must retain a resolvable texture")
            .data[..4],
        &expected
    );
}

#[test]
fn fresh_drop_preserves_other_consumers_and_undo_redo_pixels() {
    let mut f = Fixture::new();
    let mut history = EditorHistory::default();
    let scene = f.root.join("scenes/shared.ron");
    let log = scene.with_extension("editor.jsonl");
    let other = shared_red_world(&mut f, &mut history, &scene, &log);
    image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 255, 255]))
        .save(f.root.join("assets/materials/color.png"))
        .unwrap();
    let request = f.request(MaterialDropTarget::Texture(TextureUsage::Albedo));
    let (operation, prepared) = f.prepare(&request).unwrap();
    let lighting = Default::default();
    history
        .apply_prepared_material(
            &mut EditorContext::new(&mut f.world, &lighting, "shared", &scene, &log),
            operation,
            prepared,
        )
        .unwrap();
    assert_pixels(&f, f.entity, [0, 0, 255, 255]);
    assert_pixels(&f, other, [255, 0, 0, 255]);
    history
        .undo(&mut EditorContext::new(
            &mut f.world,
            &lighting,
            "shared",
            &scene,
            &log,
        ))
        .unwrap();
    assert_pixels(&f, f.entity, [255, 0, 0, 255]);
    history
        .redo(&mut EditorContext::new(
            &mut f.world,
            &lighting,
            "shared",
            &scene,
            &log,
        ))
        .unwrap();
    assert_pixels(&f, f.entity, [0, 0, 255, 255]);
    assert_pixels(&f, other, [255, 0, 0, 255]);
}

#[test]
fn failed_persistence_preserves_selected_and_other_consumers_pixels() {
    let mut f = Fixture::new();
    let mut history = EditorHistory::default();
    let scene = f.root.join("scenes/rollback.ron");
    let log = scene.with_extension("editor.jsonl");
    let other = shared_red_world(&mut f, &mut history, &scene, &log);
    let baseline = std::fs::read(&scene).unwrap();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 255, 255]))
        .save(f.root.join("assets/materials/color.png"))
        .unwrap();
    let request = f.request(MaterialDropTarget::Texture(TextureUsage::Albedo));
    let (operation, prepared) = f.prepare(&request).unwrap();
    std::fs::remove_file(&log).unwrap();
    std::fs::create_dir(&log).unwrap();
    let lighting = Default::default();
    assert!(history
        .apply_prepared_material(
            &mut EditorContext::new(&mut f.world, &lighting, "shared", &scene, &log),
            operation,
            prepared
        )
        .is_err());
    assert_eq!(std::fs::read(&scene).unwrap(), baseline);
    assert_eq!(history.operation_log().apply_count(), 1);
    assert!(f
        .world
        .query_one::<&aether_engine::ecs::components::Selected>(f.entity)
        .get()
        .is_ok());
    assert_pixels(&f, f.entity, [255, 0, 0, 255]);
    assert_pixels(&f, other, [255, 0, 0, 255]);
}
