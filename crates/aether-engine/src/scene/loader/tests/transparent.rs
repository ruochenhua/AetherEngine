use super::*;

#[test]
fn transparent_material_resolves_its_configured_albedo_texture() {
    let device = headless_device();
    let registry = test_registry();
    let mut desc = test_scene_desc();
    let texture_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/models/cyborg/cyborg_diffuse.png"
    );
    desc.objects[0].material.transparent = Some(crate::scene::TransparentMaterialConfig {
        blend: crate::renderer::transparent::TransparentBlendMode::Alpha,
        alpha_cutoff: None,
        texture: Some(texture_path.into()),
    });
    let mut world = World::new();
    let mut assets = test_assets();

    SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

    let mut transparent = world.query::<&crate::renderer::transparent::TransparentMaterial>();
    let material = transparent
        .iter()
        .next()
        .expect("transparent material loaded");
    assert!(material.texture.is_some());
}
