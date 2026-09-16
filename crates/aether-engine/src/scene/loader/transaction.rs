//! Transactional scene replacement helpers.

use crate::{
    asset::{registry::BuiltinMeshRegistry, AssetManager},
    ecs::World,
    renderer::light::LightingUniforms,
    scene::SceneDescription,
};

/// Build a scene in isolation and commit it only after construction succeeds.
pub(super) fn replace_scene(
    desc: &SceneDescription,
    device: &wgpu::Device,
    registry: &BuiltinMeshRegistry,
    assets: &mut AssetManager,
    world: &mut World,
) -> anyhow::Result<LightingUniforms> {
    replace_world_on_success(world, |next_world| {
        super::SceneLoader::build_world(desc, device, registry, assets, next_world)
    })
}

/// Commit a newly built world only after the builder has completed
/// successfully. This prevents partially loaded scenes from replacing a
/// usable world when an object or asset fails during construction.
pub(super) fn replace_world_on_success<T, F>(world: &mut World, build: F) -> anyhow::Result<T>
where
    F: FnOnce(&mut World) -> anyhow::Result<T>,
{
    let mut next_world = World::new();
    let result = build(&mut next_world)?;
    *world = next_world;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::replace_world_on_success;
    use crate::ecs::components::{Name, Transform};
    use crate::ecs::World;

    #[test]
    fn failed_build_keeps_existing_world() {
        let mut world = World::new();
        world.spawn((Transform::default(), Name("old".into())));

        let result = replace_world_on_success(&mut world, |next| -> anyhow::Result<()> {
            next.spawn((Transform::default(), Name("partial".into())));
            Err(anyhow::anyhow!("scene build failed"))
        });

        assert!(result.is_err());
        assert_eq!(world.len(), 1);
        let names: Vec<_> = world
            .query::<&Name>()
            .iter()
            .map(|name| name.0.clone())
            .collect();
        assert_eq!(names, vec!["old".to_string()]);
    }
}
