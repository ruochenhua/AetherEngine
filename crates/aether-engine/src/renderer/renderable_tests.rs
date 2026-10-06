use super::{MaterialUniform, ObjectUniform};
use crate::asset::AssetManager;
use crate::scene::material::MaterialResolver;
use crate::scene::MaterialConfig;

#[test]
fn material_uniform_adapter_preserves_resolved_pbr_values() {
    let config = MaterialConfig {
        normal_scale: 1.5,
        occlusion_strength: 0.4,
        emissive: [0.2, 0.3, 0.4],
        emissive_intensity: 2.0,
        unlit: true,
        ..MaterialConfig::default()
    };
    let resolution = MaterialResolver::new("project")
        .resolve(&config, &mut AssetManager::new())
        .expect("material should resolve");

    let uniform = MaterialUniform::from_resolution(&resolution);

    assert_eq!(uniform.normal_scale, 1.5);
    assert_eq!(uniform.occlusion_strength, 0.4);
    assert_eq!(uniform.emissive, [0.2, 0.3, 0.4]);
    assert_eq!(uniform.emissive_intensity, 2.0);
    assert_eq!(uniform.unlit, 1);
    assert_eq!(uniform.normal_texture_id, 0);
    assert_eq!(uniform.orm_texture_id, 0);
    assert_eq!(uniform.emissive_texture_id, 0);
}

#[test]
fn object_uniform_carries_extended_texture_presence_flags() {
    let material = MaterialUniform {
        normal_scale: 1.25,
        occlusion_strength: 0.6,
        normal_texture_id: 7,
        orm_texture_id: 8,
        emissive_texture_id: 9,
        ..MaterialUniform::default()
    };

    let object = ObjectUniform::from_material(&material);

    assert_eq!(object.normal_scale, 1.25);
    assert_eq!(object.occlusion_strength, 0.6);
    assert_eq!(object.normal_present, 1);
    assert_eq!(object.orm_present, 1);
    assert_eq!(object.emissive_present, 1);
}

#[test]
fn extracted_object_uniform_packs_configured_orm_channels() {
    use crate::renderer::extracted_material::ExtractedMaterial;
    use crate::scene::config::material::{OrmSwizzle, TextureChannel};

    let config = MaterialConfig {
        orm_swizzle: OrmSwizzle {
            ao: TextureChannel::A,
            roughness: TextureChannel::Zero,
            metallic: TextureChannel::One,
        },
        ..MaterialConfig::default()
    };
    let resolution = MaterialResolver::new("project")
        .resolve(&config, &mut AssetManager::new())
        .unwrap();
    let extracted = ExtractedMaterial::from_resolution(&resolution);

    let object = ObjectUniform::from_extracted_material(&extracted);

    assert_eq!(object.orm_swizzle, 3 | (4 << 3) | (5 << 6));
}
