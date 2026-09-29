use super::*;

#[test]
fn build_world_no_terrain_entity_when_missing() {
    let device = headless_device();
    let registry = test_registry();
    let desc = test_scene_desc();
    let mut world = World::new();
    let mut assets = test_assets();
    SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

    let count = world
        .query::<&crate::ecs::components::Terrain>()
        .iter()
        .count();
    assert_eq!(count, 0);
}

#[test]
fn build_world_spawns_terrain_entity_when_configured() {
    let device = headless_device();
    let registry = test_registry();
    let mut desc = test_scene_desc();
    desc.terrain = Some(TerrainConfig {
        source: TerrainSource::Procedural {
            seed: 0,
            frequency: 0.1,
            amplitude: 1.0,
        },
        geometry: TerrainGeometry {
            extent: 128.0,
            chunk_size: 32,
            max_lod: 2,
            albedo_tiling: 64.0,
        },
        splatmap: None,
        layers: vec![TerrainLayerConfig::default()],
    });
    let mut world = World::new();
    let mut assets = test_assets();
    SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

    let count = world
        .query::<&crate::ecs::components::Terrain>()
        .iter()
        .count();
    assert_eq!(count, 1);
}

#[test]
fn build_world_spawns_atmosphere_entity_when_configured() {
    let device = headless_device();
    let registry = test_registry();
    let mut desc = test_scene_desc();
    desc.atmosphere = Some(AtmosphereConfig::default());
    let mut world = World::new();
    let mut assets = test_assets();
    SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

    let count = world
        .query::<&crate::ecs::components::Atmosphere>()
        .iter()
        .count();
    assert_eq!(count, 1);
}

#[test]
fn build_world_spawns_water_entity_when_configured() {
    let device = headless_device();
    let registry = test_registry();
    let mut desc = test_scene_desc();
    desc.water = Some(WaterConfig::default());
    let mut world = World::new();
    let mut assets = test_assets();
    SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

    let count = world
        .query::<&crate::ecs::components::Water>()
        .iter()
        .count();
    assert_eq!(count, 1);
}
