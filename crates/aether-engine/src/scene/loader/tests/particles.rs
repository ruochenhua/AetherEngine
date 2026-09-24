use super::*;

#[test]
fn build_world_spawns_particle_emitters_with_runtime_entity_keys() {
    let device = headless_device();
    let registry = test_registry();
    let mut assets = test_assets();
    let mut world = World::new();
    let mut desc = test_scene_desc();
    desc.particle_emitters
        .push(crate::particles::ParticleEmitterConfig {
            seed: 912,
            emission_rate: 24.0,
            ..Default::default()
        });

    SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

    let emitters: Vec<_> = world
        .query::<(crate::ecs::Entity, &crate::particles::ParticleEmitterConfig)>()
        .iter()
        .map(|(entity, config)| (entity.to_bits().get(), config.entity_bits, config.seed))
        .collect();
    assert_eq!(emitters.len(), 1);
    assert_eq!(emitters[0].2, 912);
    assert_ne!(emitters[0].0, 0);
    assert_eq!(emitters[0].0, emitters[0].1);
}
