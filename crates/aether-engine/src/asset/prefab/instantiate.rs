//! Runtime materialization of a validated Prefab document.

use super::{PrefabComponentKind, PrefabDocument, PrefabError, PrefabInstanceConfig};
use crate::ecs::components::{
    Atmosphere, Camera, Clouds, Light, MeshHandle, Name, PrefabInstanceRoot, PrefabNodeInstance,
    Transform, Visibility,
};
use crate::ecs::{Entity, World};
use crate::editor::ComponentRecord;
use crate::renderer::renderable::MaterialUniform;
use crate::renderer::transparent::TransparentMaterial;
use crate::scene::{MaterialConfig, MeshRef};
use glam::{Quat, Vec3};
use std::collections::BTreeMap;

/// Resolved runtime values for one prefab material record.
pub struct ResolvedPrefabMaterial {
    /// GPU-ready material parameters.
    pub uniform: MaterialUniform,
    /// Optional transparent pass configuration.
    pub transparent: Option<TransparentMaterial>,
}

/// Runtime asset seam used to resolve closed Prefab mesh and material records.
pub trait PrefabAssets {
    /// Resolve a persisted mesh reference into a runtime GPU mesh handle.
    fn resolve_mesh(&mut self, source: &MeshRef) -> Result<MeshHandle, PrefabError>;

    /// Resolve a material and all its texture dependencies.
    fn resolve_material(
        &mut self,
        config: &MaterialConfig,
    ) -> Result<ResolvedPrefabMaterial, PrefabError>;
}

/// Inputs needed to instantiate one placed prefab instance.
pub struct PrefabContext<'a> {
    /// Stable scene instance id, asset path, and per-instance overrides.
    pub instance: &'a PrefabInstanceConfig,
    /// Asset resolver owned by the scene-loading caller.
    pub assets: &'a mut dyn PrefabAssets,
}

impl<'a> PrefabContext<'a> {
    /// Construct a Prefab context from a scene reference and resolver.
    pub fn new(instance: &'a PrefabInstanceConfig, assets: &'a mut dyn PrefabAssets) -> Self {
        Self { instance, assets }
    }
}

/// Instantiate a Prefab transactionally and return its stable node-id map.
///
/// All schema checks, overrides, and asset resolution complete before the World
/// is mutated. If an ECS insertion fails, every entity created by this call is
/// despawned before returning the error.
pub fn instantiate(
    document: &PrefabDocument,
    context: &mut PrefabContext<'_>,
    world: &mut World,
) -> Result<BTreeMap<u64, Entity>, PrefabError> {
    instantiate_inner(document, context, world, None)
}

#[cfg(test)]
pub(super) fn instantiate_with_insert_failure_after(
    document: &PrefabDocument,
    context: &mut PrefabContext<'_>,
    world: &mut World,
    successful_insertions: usize,
) -> Result<BTreeMap<u64, Entity>, PrefabError> {
    instantiate_inner(document, context, world, Some(successful_insertions))
}

fn instantiate_inner(
    document: &PrefabDocument,
    context: &mut PrefabContext<'_>,
    world: &mut World,
    fail_after_insertions: Option<usize>,
) -> Result<BTreeMap<u64, Entity>, PrefabError> {
    document.validate()?;
    if context.instance.prefab_asset.trim().is_empty() {
        return Err(PrefabError::Dependency(
            "prefab asset path must not be empty".to_string(),
        ));
    }
    if world
        .query::<&PrefabInstanceRoot>()
        .iter()
        .any(|root| root.0.instance_id == context.instance.instance_id)
    {
        return Err(PrefabError::DuplicateSceneInstanceId(
            context.instance.instance_id,
        ));
    }

    let mut resolved = document.clone();
    context.instance.overrides.apply_to(&mut resolved)?;
    let mut prepared = Vec::new();
    prepare_node(&resolved.root, None, context, &mut prepared)?;

    let mut created = Vec::with_capacity(prepared.len());
    let mut entities = BTreeMap::new();
    for (index, node) in prepared.into_iter().enumerate() {
        let entity = world.spawn(());
        created.push(entity);
        entities.insert(node.instance_id, entity);
        let result = (|| {
            world
                .insert(
                    entity,
                    (PrefabNodeInstance {
                        prefab_instance_id: context.instance.instance_id,
                        instance_id: node.instance_id,
                        parent_instance_id: node.parent_instance_id,
                    },),
                )
                .map_err(|error| error.to_string())?;
            if index == 0 {
                world
                    .insert(entity, (PrefabInstanceRoot(context.instance.clone()),))
                    .map_err(|error| error.to_string())?;
            }
            for (inserted, component) in node.components.into_iter().enumerate() {
                if fail_after_insertions == Some(inserted) {
                    return Err("injected component insertion failure".to_string());
                }
                insert_component(world, entity, component)?;
            }
            Ok::<(), String>(())
        })();
        if let Err(error) = result {
            rollback(world, &created)?;
            return Err(PrefabError::Dependency(error));
        }
    }
    Ok(entities)
}

struct PreparedNode {
    instance_id: u64,
    parent_instance_id: Option<u64>,
    components: Vec<RuntimeComponent>,
}

enum RuntimeComponent {
    Transform(Transform),
    Mesh(MeshHandle),
    Material(Box<MaterialConfig>, ResolvedPrefabMaterial),
    Visibility(Visibility),
    Name(Name),
    Light(Light),
    Camera(Camera),
    Atmosphere(Atmosphere),
    Clouds(Clouds),
}

fn prepare_node(
    node: &super::PrefabNode,
    parent_instance_id: Option<u64>,
    context: &mut PrefabContext<'_>,
    output: &mut Vec<PreparedNode>,
) -> Result<(), PrefabError> {
    let removed = |kind| {
        context
            .instance
            .overrides
            .removed_components
            .iter()
            .any(|entry| entry.instance_id == node.instance_id && entry.component == kind)
    };
    let mut components = Vec::with_capacity(node.components.len() + 2);
    let mut has_mesh = false;
    let mut has_material = false;
    let mut has_name = false;
    for record in &node.components {
        match record {
            ComponentRecord::Transform {
                translation,
                rotation_xyzw,
                scale,
            } => components.push(RuntimeComponent::Transform(Transform {
                translation: Vec3::from_array(*translation),
                rotation: Quat::from_array(*rotation_xyzw),
                scale: Vec3::from_array(*scale),
            })),
            ComponentRecord::Mesh { source } => {
                has_mesh = true;
                components.push(RuntimeComponent::Mesh(context.assets.resolve_mesh(source)?));
            }
            ComponentRecord::Material { config } => {
                has_material = true;
                components.push(RuntimeComponent::Material(
                    Box::new(config.clone()),
                    context.assets.resolve_material(config)?,
                ));
            }
            ComponentRecord::Visibility { visible } => {
                components.push(RuntimeComponent::Visibility(Visibility(*visible)))
            }
            ComponentRecord::Name { value } => {
                has_name = true;
                components.push(RuntimeComponent::Name(Name(value.clone())))
            }
            ComponentRecord::Light { config } => components.push(RuntimeComponent::Light(Light {
                light_type: config.light_type,
                color: config.color,
                intensity: config.intensity,
                range: config.range,
                inner_cone_angle: config.inner_cone_angle,
                outer_cone_angle: config.outer_cone_angle,
                cast_shadow: config.light_type == crate::renderer::light::LightType::Directional,
            })),
            ComponentRecord::Camera { config } => {
                components.push(RuntimeComponent::Camera(Camera {
                    fov: config.fov.to_radians(),
                    near: config.near,
                    far: config.far,
                    speed: config.speed,
                }))
            }
            ComponentRecord::Atmosphere { config } => {
                components.push(RuntimeComponent::Atmosphere(Atmosphere {
                    config: config.clone(),
                }))
            }
            ComponentRecord::Clouds { config } => {
                components.push(RuntimeComponent::Clouds(Clouds {
                    config: config.clone(),
                }))
            }
        }
    }
    if has_mesh && !has_material && !removed(PrefabComponentKind::Material) {
        let config = MaterialConfig::default();
        components.push(RuntimeComponent::Material(
            Box::new(config.clone()),
            context.assets.resolve_material(&config)?,
        ));
    }
    if has_mesh
        && !node
            .components
            .iter()
            .any(|record| matches!(record, ComponentRecord::Visibility { .. }))
        && !removed(PrefabComponentKind::Visibility)
    {
        components.push(RuntimeComponent::Visibility(Visibility::default()));
    }
    if !has_name && !removed(PrefabComponentKind::Name) {
        components.push(RuntimeComponent::Name(Name(node.name.clone())));
    }
    output.push(PreparedNode {
        instance_id: node.instance_id,
        parent_instance_id,
        components,
    });
    for child in &node.children {
        prepare_node(child, Some(node.instance_id), context, output)?;
    }
    Ok(())
}

fn insert_component(
    world: &mut World,
    entity: Entity,
    component: RuntimeComponent,
) -> Result<(), String> {
    match component {
        RuntimeComponent::Transform(value) => insert_one(world, entity, (value,)),
        RuntimeComponent::Mesh(value) => insert_one(world, entity, (value,)),
        RuntimeComponent::Material(config, resolved) => {
            insert_one(world, entity, (*config, resolved.uniform))?;
            if let Some(transparent) = resolved.transparent {
                insert_one(world, entity, (transparent,))?;
            }
            Ok(())
        }
        RuntimeComponent::Visibility(value) => insert_one(world, entity, (value,)),
        RuntimeComponent::Name(value) => insert_one(world, entity, (value,)),
        RuntimeComponent::Light(value) => insert_one(world, entity, (value,)),
        RuntimeComponent::Camera(value) => insert_one(world, entity, (value,)),
        RuntimeComponent::Atmosphere(value) => insert_one(world, entity, (value,)),
        RuntimeComponent::Clouds(value) => insert_one(world, entity, (value,)),
    }
}

fn insert_one<B: hecs::DynamicBundle>(
    world: &mut World,
    entity: Entity,
    component: B,
) -> Result<(), String> {
    world
        .insert(entity, component)
        .map_err(|error| error.to_string())
}

fn rollback(world: &mut World, created: &[Entity]) -> Result<(), PrefabError> {
    for entity in created.iter().rev() {
        world
            .despawn(*entity)
            .map_err(|error| PrefabError::RollbackFailed(error.to_string()))?;
    }
    Ok(())
}
