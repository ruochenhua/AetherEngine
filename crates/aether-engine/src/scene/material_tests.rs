use super::super::config::MaterialConfig;
use crate::asset::AssetManager;

#[test]
fn legacy_material_defaults_resolve_to_the_frozen_opaque_contract() {
    let config = MaterialConfig::default();
    let resolution = super::MaterialResolver::new("project")
        .resolve(&config, &mut AssetManager::new())
        .expect("legacy defaults should resolve");

    assert_eq!(resolution.material.base_color, [0.8, 0.8, 0.8, 1.0]);
    assert_eq!(resolution.material.metallic, 0.0);
    assert_eq!(resolution.material.roughness, 0.5);
    assert_eq!(resolution.material.normal_scale, 1.0);
    assert_eq!(resolution.material.occlusion_strength, 1.0);
    assert_eq!(resolution.material.emissive, [0.0, 0.0, 0.0]);
    assert_eq!(resolution.material.emissive_intensity, 1.0);
    assert_eq!(resolution.albedo.fallback, super::TextureFallback::White);
    assert_eq!(
        resolution.normal.fallback,
        super::TextureFallback::FlatNormal
    );
    assert_eq!(resolution.orm.fallback, super::TextureFallback::White);
    assert_eq!(resolution.emissive.fallback, super::TextureFallback::Black);
}

#[test]
fn resolver_preserves_legacy_unlit_for_the_flag_off_adapter() {
    let config = MaterialConfig {
        unlit: true,
        ..MaterialConfig::default()
    };
    let resolution = super::MaterialResolver::new("project")
        .resolve(&config, &mut AssetManager::new())
        .expect("legacy unlit material should resolve");

    assert!(resolution.legacy_unlit);
}

#[test]
fn missing_texture_paths_keep_usage_specific_fallbacks() {
    let config = MaterialConfig {
        albedo_texture: Some("textures/albedo.png".into()),
        normal_texture: Some("textures/normal.png".into()),
        orm_texture: Some("textures/orm.png".into()),
        emissive_texture: Some("textures/emissive.png".into()),
        ..MaterialConfig::default()
    };
    let resolution = super::MaterialResolver::new("project")
        .resolve(&config, &mut AssetManager::new())
        .expect("missing textures should resolve with fallbacks");

    assert_eq!(resolution.albedo.fallback, super::TextureFallback::White);
    assert_eq!(
        resolution.normal.fallback,
        super::TextureFallback::FlatNormal
    );
    assert_eq!(resolution.orm.fallback, super::TextureFallback::White);
    assert_eq!(resolution.emissive.fallback, super::TextureFallback::Black);
    assert!(resolution.albedo.handle.is_none());
    assert!(resolution.normal.handle.is_none());
    assert!(resolution.orm.handle.is_none());
    assert!(resolution.emissive.handle.is_none());
}

#[test]
fn texture_cache_key_separates_usage_and_color_space() {
    let config = MaterialConfig {
        albedo_texture: Some("textures/../textures/shared.png".into()),
        ..MaterialConfig::default()
    };
    let resolution = super::MaterialResolver::new("project")
        .resolve(&config, &mut AssetManager::new())
        .unwrap();
    let albedo_key = resolution.albedo.key.clone();
    let normal_key =
        super::TextureCacheKey::new(albedo_key.path().to_owned(), super::TextureUsage::Normal);

    assert_eq!(albedo_key.path(), "project/textures/shared.png");
    assert_ne!(albedo_key, normal_key);
    assert_eq!(albedo_key.color_space(), super::ColorSpace::Srgb);
    assert_eq!(normal_key.color_space(), super::ColorSpace::Linear);
}

#[test]
fn invalid_material_ranges_fail_closed() {
    let config = MaterialConfig {
        normal_scale: 2.1,
        ..MaterialConfig::default()
    };
    let error = super::MaterialResolver::new("project")
        .resolve(&config, &mut AssetManager::new())
        .expect_err("normal scale outside the contract must fail");
    assert!(matches!(
        error,
        super::MaterialResolveError::OutOfRange {
            field: "normal_scale",
            ..
        }
    ));

    let config = MaterialConfig {
        emissive_intensity: -1.0,
        ..MaterialConfig::default()
    };
    let error = super::MaterialResolver::new("project")
        .resolve(&config, &mut AssetManager::new())
        .expect_err("negative emissive intensity must fail");
    assert!(matches!(
        error,
        super::MaterialResolveError::OutOfRange {
            field: "emissive_intensity",
            ..
        }
    ));
}

#[test]
fn orm_defaults_are_explicit_and_stable() {
    let config = MaterialConfig::default();
    assert_eq!(config.orm_swizzle, super::OrmSwizzle::default());
    assert_eq!(
        config.orm_swizzle.ao,
        crate::scene::config::material::TextureChannel::R
    );
    assert_eq!(
        config.orm_swizzle.roughness,
        crate::scene::config::material::TextureChannel::G
    );
    assert_eq!(
        config.orm_swizzle.metallic,
        crate::scene::config::material::TextureChannel::B
    );
}

#[test]
fn extended_ron_material_preserves_texture_usage_metadata() {
    let scene = crate::scene::SceneDescription::from_ron(
        r#"SceneDescription(
            name: "Extended Material",
            camera: (position: (0.0, 0.0, 2.0)),
            objects: [
                (
                    name: "PbrObject",
                    mesh: Builtin("cube"),
                    material: (
                        normal_texture: Some("textures/normal.png"),
                        orm_texture: Some("textures/orm.png"),
                        emissive_texture: Some("textures/emissive.png"),
                        normal_scale: 1.25,
                        occlusion_strength: 0.75,
                        emissive: (0.1, 0.2, 0.3),
                        emissive_intensity: 3.0,
                        orm_swizzle: (ao: A, roughness: R, metallic: G),
                    ),
                ),
            ],
        )"#,
    )
    .expect("extended material should parse");

    let material = &scene.objects[0].material;
    assert_eq!(
        material.normal_texture.as_deref(),
        Some("textures/normal.png")
    );
    assert_eq!(material.orm_texture.as_deref(), Some("textures/orm.png"));
    assert_eq!(
        material.emissive_texture.as_deref(),
        Some("textures/emissive.png")
    );
    assert_eq!(material.normal_scale, 1.25);
    assert_eq!(material.occlusion_strength, 0.75);
    assert_eq!(material.emissive, [0.1, 0.2, 0.3]);
    assert_eq!(material.emissive_intensity, 3.0);
    assert_eq!(
        material.orm_swizzle.ao,
        crate::scene::config::material::TextureChannel::A
    );
    assert_eq!(
        material.orm_swizzle.roughness,
        crate::scene::config::material::TextureChannel::R
    );
    assert_eq!(
        material.orm_swizzle.metallic,
        crate::scene::config::material::TextureChannel::G
    );
}
