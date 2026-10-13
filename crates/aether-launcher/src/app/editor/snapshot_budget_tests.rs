use super::{material_drop::prepare_material_drop, material_drop_tests::Fixture};
use crate::app::workspace::MaterialDropTarget;
use aether_engine::{
    asset::{AssetManager, TextureSnapshotLimits},
    scene::{material::TextureUsage, MaterialConfig},
};
use std::fs;

#[test]
fn invalid_scalar_and_later_dependency_never_register_rewritten_pixels() {
    let mut f = Fixture::new();
    let request = f.request(MaterialDropTarget::Asset);
    let (_, initial) = f.prepare(&request).unwrap();
    let old = initial.extracted_material.albedo_texture().unwrap();
    let stats = (
        f.assets.texture_snapshot_count(),
        f.assets.texture_snapshot_bytes(),
    );
    image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 255, 255]))
        .save(f.root.join("assets/materials/color.png"))
        .unwrap();
    for config in [
        MaterialConfig {
            albedo_texture: Some("color.png".into()),
            normal_scale: 4.0,
            ..Default::default()
        },
        MaterialConfig {
            albedo_texture: Some("color.png".into()),
            normal_texture: Some("missing.png".into()),
            ..Default::default()
        },
        MaterialConfig {
            albedo_texture: Some("color.png".into()),
            transparent: Some(aether_engine::scene::TransparentMaterialConfig {
                alpha_cutoff: Some(2.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    ] {
        fs::write(
            f.root.join("assets/materials/sample.ron"),
            ron::ser::to_string(&config).unwrap(),
        )
        .unwrap();
        assert!(f.prepare(&request).is_err());
        assert_eq!(
            (
                f.assets.texture_snapshot_count(),
                f.assets.texture_snapshot_bytes()
            ),
            stats
        );
        assert!(f.assets.get(old).is_some());
    }
}

#[test]
fn over_budget_material_batch_keeps_cache_and_previous_handles_unchanged() {
    let mut f = Fixture::new();
    f.assets = AssetManager::with_project_root_and_texture_snapshot_limits(
        &f.root,
        TextureSnapshotLimits {
            decoded_bytes: 20,
            count: 16,
        },
    );
    let request = f.request(MaterialDropTarget::Asset);
    let (_, initial) = f.prepare(&request).unwrap();
    let old = initial.extracted_material.albedo_texture().unwrap();
    assert_eq!(
        (
            f.assets.texture_snapshot_count(),
            f.assets.texture_snapshot_bytes()
        ),
        (1, 16)
    );
    image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 255, 255]))
        .save(f.root.join("assets/materials/color.png"))
        .unwrap();
    image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 255, 0, 255]))
        .save(f.root.join("assets/materials/normal.png"))
        .unwrap();
    let config = MaterialConfig {
        albedo_texture: Some("color.png".into()),
        normal_texture: Some("normal.png".into()),
        ..Default::default()
    };
    fs::write(
        f.root.join("assets/materials/sample.ron"),
        ron::ser::to_string(&config).unwrap(),
    )
    .unwrap();
    assert!(f.prepare(&request).is_err());
    assert_eq!(
        (
            f.assets.texture_snapshot_count(),
            f.assets.texture_snapshot_bytes()
        ),
        (1, 16)
    );
    assert_eq!(&f.assets.get(old).unwrap().data[..4], &[128, 64, 32, 255]);
}

#[test]
fn valid_large_image_drop_is_accepted_with_exact_snapshot_accounting() {
    let mut f = Fixture::new();
    image::RgbaImage::from_pixel(1024, 1024, image::Rgba([0, 0, 255, 255]))
        .save(f.root.join("assets/materials/color.png"))
        .unwrap();
    let request = f.request(MaterialDropTarget::Texture(TextureUsage::Albedo));
    let (_, prepared) = prepare_material_drop(&request, &f.root, &mut f.assets, &f.world).unwrap();
    let texture = f
        .assets
        .get(prepared.extracted_material.albedo_texture().unwrap())
        .unwrap();
    assert_eq!((texture.width, texture.height), (1024, 1024));
    assert_eq!(
        (
            f.assets.texture_snapshot_count(),
            f.assets.texture_snapshot_bytes()
        ),
        (1, 1024 * 1024 * 4)
    );
}

#[test]
fn every_material_scalar_is_validated_before_snapshot_registration() {
    let mut f = Fixture::new();
    let base = MaterialConfig {
        albedo_texture: Some("assets/materials/color.png".into()),
        ..Default::default()
    };
    for material in [
        MaterialConfig {
            albedo: [f32::NAN, 1.0, 1.0, 1.0],
            ..base.clone()
        },
        MaterialConfig {
            emissive: [f32::INFINITY, 0.0, 0.0],
            ..base.clone()
        },
        MaterialConfig {
            roughness: f32::INFINITY,
            ..base.clone()
        },
        MaterialConfig {
            roughness: -0.1,
            ..base.clone()
        },
        MaterialConfig {
            metallic: 2.0,
            ..base.clone()
        },
    ] {
        let assignment = aether_engine::editor::MaterialAssignment::Inline { material };
        assert!(super::material_drop::resolve_material_assignment(
            &assignment,
            &f.root,
            &mut f.assets
        )
        .is_err());
        assert_eq!(
            (
                f.assets.texture_snapshot_count(),
                f.assets.texture_snapshot_bytes()
            ),
            (0, 0)
        );
    }
}
