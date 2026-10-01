//! Object spawning helpers for `SceneLoader`.

use crate::physics::{Collider, ColliderList, ColliderShape, RigidBody};
use crate::{
    asset::{
        mesh::{CpuMesh, GpuMesh},
        registry::BuiltinMeshRegistry,
        AssetManager,
    },
    ecs::components::{MeshHandle, MeshSource, Name, Transform, Visibility},
    ecs::World,
    renderer::renderable::MaterialUniform,
    renderer::transparent::TransparentMaterial,
    scene::{
        material::MaterialResolver, MaterialConfig, MeshRef, PhysicsColliderShapeConfig,
        PhysicsConfig, SceneDescription,
    },
};
use glam::{Quat, Vec3};
use std::collections::HashMap;
use std::sync::Arc;

/// Spawn object entities from `SceneDescription.objects`.
pub(super) fn build_objects(
    desc: &SceneDescription,
    device: &wgpu::Device,
    registry: &BuiltinMeshRegistry,
    assets: &mut AssetManager,
    world: &mut World,
) -> anyhow::Result<()> {
    let mut mesh_cache: HashMap<String, Arc<GpuMesh>> = HashMap::new();
    let material_resolver = MaterialResolver::new(".");

    for obj in &desc.objects {
        let mesh_source = match &obj.mesh {
            MeshRef::Builtin(name) => MeshSource::Builtin(name.clone()),
            MeshRef::File(path) => MeshSource::File(path.clone()),
        };
        let cache_key = match &obj.mesh {
            MeshRef::Builtin(name) => name.clone(),
            MeshRef::File(path) => path.clone(),
        };

        let cpu_mesh: Option<Arc<CpuMesh>> = match &obj.mesh {
            MeshRef::Builtin(name) => {
                if !mesh_cache.contains_key(&cache_key) {
                    let cpu_mesh = registry
                        .get(name)
                        .ok_or_else(|| anyhow::anyhow!("Unknown built-in mesh: '{}'", name))?
                        .clone();
                    let gpu_mesh = Arc::new(GpuMesh::from_cpu(device, &cpu_mesh));
                    mesh_cache.insert(cache_key.clone(), gpu_mesh);
                }
                None
            }
            MeshRef::File(path) => {
                let handle = assets
                    .load::<CpuMesh>(path)
                    .map_err(|e| anyhow::anyhow!("Failed to load mesh '{}': {}", path, e))?;
                let cpu_mesh = assets.get(handle).ok_or_else(|| {
                    anyhow::anyhow!("Loaded mesh not found in asset manager: '{}'", path)
                })?;
                if !mesh_cache.contains_key(&cache_key) {
                    let gpu_mesh = Arc::new(GpuMesh::from_cpu(device, &cpu_mesh));
                    mesh_cache.insert(cache_key.clone(), gpu_mesh);
                }
                Some(cpu_mesh)
            }
        };

        let base_gpu_mesh = mesh_cache.get(&cache_key).unwrap().clone();
        let mesh_name = cache_key;

        let transform = Transform {
            translation: Vec3::from_array(obj.transform.translation),
            rotation: Quat::from_array(obj.transform.rotation),
            scale: Vec3::from_array(obj.transform.scale),
        };

        if let Some(physics) = &obj.physics {
            validate_physics_config(&obj.name, physics, &transform)?;
        }

        if obj.physics.is_some() && matches!(obj.mesh, MeshRef::File(_)) {
            anyhow::bail!(
                "Physics on file meshes is not supported yet (object '{}'); use a built-in mesh",
                obj.name
            );
        }

        // If the loaded file mesh defines per-material submeshes, spawn one
        // entity per submesh so each part can use its own albedo texture.
        // Otherwise fall back to the single material defined in the scene.
        if let Some(cpu_mesh) = cpu_mesh {
            if !cpu_mesh.submeshes.is_empty() {
                for submesh in &cpu_mesh.submeshes {
                    let gpu_mesh = Arc::new(GpuMesh::submesh_view(
                        &base_gpu_mesh,
                        submesh.index_offset as u32,
                        submesh.index_count as u32,
                    ));

                    let material_config = MaterialConfig {
                        albedo: submesh.material.base_color,
                        roughness: submesh.material.roughness,
                        metallic: submesh.material.metallic,
                        albedo_texture: submesh.material.albedo_texture.clone(),
                        ..MaterialConfig::default()
                    };
                    let resolution = material_resolver.resolve(&material_config, assets)?;
                    let material = MaterialUniform::from_resolution(&resolution);

                    let entity = world.spawn((
                        transform.clone(),
                        MeshHandle::new(
                            gpu_mesh,
                            mesh_source.clone(),
                            format!("{}::{}", mesh_name, submesh.name),
                        ),
                        material_config,
                        material,
                        Visibility(obj.visible),
                        Name(format!("{}::{}", obj.name, submesh.name)),
                    ));
                    attach_physics(world, entity, obj.physics.as_ref())?;
                }
                continue;
            }
        }

        let mut material_config = obj.material.clone();
        if let Some(texture) = obj
            .material
            .transparent
            .as_ref()
            .and_then(|transparent| transparent.texture.as_ref())
        {
            material_config.albedo_texture = Some(texture.clone());
        }
        let resolution = material_resolver.resolve(&material_config, assets)?;
        let material = MaterialUniform::from_resolution(&resolution);
        let mesh_handle = MeshHandle::new(base_gpu_mesh, mesh_source, mesh_name);

        if let Some(config) = &obj.material.transparent {
            let transparent_material = TransparentMaterial {
                base_color: obj.material.albedo,
                texture: resolution.material.albedo,
                blend: config.blend,
                alpha_cutoff: config.alpha_cutoff,
            };
            transparent_material.validate().map_err(|error| {
                anyhow::anyhow!("Invalid transparent material '{}': {:?}", obj.name, error)
            })?;
            let entity = world.spawn((
                transform,
                mesh_handle,
                obj.material.clone(),
                material,
                transparent_material,
                Visibility(obj.visible),
                Name(obj.name.clone()),
            ));
            attach_physics(world, entity, obj.physics.as_ref())?;
        } else {
            let entity = world.spawn((
                transform,
                mesh_handle,
                obj.material.clone(),
                material,
                Visibility(obj.visible),
                Name(obj.name.clone()),
            ));
            attach_physics(world, entity, obj.physics.as_ref())?;
        }
    }
    Ok(())
}

fn attach_physics(
    world: &mut World,
    entity: crate::ecs::Entity,
    config: Option<&PhysicsConfig>,
) -> anyhow::Result<()> {
    let Some(config) = config else {
        return Ok(());
    };
    let colliders = config
        .colliders
        .iter()
        .map(|collider| {
            let shape = match collider.shape {
                PhysicsColliderShapeConfig::Sphere { radius } => ColliderShape::Sphere(radius),
                PhysicsColliderShapeConfig::Box { half_extents } => {
                    ColliderShape::Box(Vec3::from_array(half_extents))
                }
                PhysicsColliderShapeConfig::Capsule { radius, height } => {
                    ColliderShape::Capsule(radius, height)
                }
                PhysicsColliderShapeConfig::Mesh => ColliderShape::Mesh,
            };
            Collider {
                shape,
                is_trigger: collider.is_trigger,
                friction: collider.friction,
                restitution: collider.restitution,
            }
        })
        .collect();
    world.insert(
        entity,
        (
            RigidBody {
                velocity: Vec3::from_array(config.body.velocity),
                angular_velocity: Vec3::from_array(config.body.angular_velocity),
                mass: config.body.mass,
                is_static: config.body.is_static,
            },
            ColliderList(colliders),
        ),
    )?;
    Ok(())
}

fn validate_physics_config(
    name: &str,
    config: &PhysicsConfig,
    transform: &Transform,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        !config.colliders.is_empty() && config.colliders.len() <= 4,
        "Invalid physics on object '{name}': expected 1 to 4 colliders"
    );
    anyhow::ensure!(
        config.body.mass.is_finite()
            && (config.body.is_static || config.body.mass > 0.0)
            && config.body.velocity.iter().all(|value| value.is_finite())
            && config
                .body
                .angular_velocity
                .iter()
                .all(|value| value.is_finite()),
        "Invalid physics on object '{name}': mass and velocities must be finite; dynamic mass must be positive"
    );
    anyhow::ensure!(
        transform.translation.is_finite()
            && transform.rotation.is_finite()
            && transform.rotation.length_squared() > f32::EPSILON
            && transform.scale.is_finite()
            && transform.scale.min_element() > 0.0
            && (transform.scale.x - transform.scale.y).abs() <= 1.0e-5
            && (transform.scale.x - transform.scale.z).abs() <= 1.0e-5,
        "Invalid physics transform on object '{name}': translation/rotation must be finite and scale must be positive, finite, and uniform"
    );
    for collider in &config.colliders {
        anyhow::ensure!(
            collider.friction.is_finite()
                && collider.friction >= 0.0
                && collider.restitution.is_finite()
                && (0.0..=1.0).contains(&collider.restitution),
            "Invalid physics material on object '{name}': friction must be non-negative and restitution in [0, 1]"
        );
        match collider.shape {
            PhysicsColliderShapeConfig::Sphere { radius } => anyhow::ensure!(
                radius.is_finite() && radius > 0.0,
                "Invalid sphere collider on object '{name}': radius must be positive and finite"
            ),
            PhysicsColliderShapeConfig::Box { half_extents } => anyhow::ensure!(
                half_extents
                    .iter()
                    .all(|extent| extent.is_finite() && *extent > 0.0),
                "Invalid box collider on object '{name}': half extents must be positive and finite"
            ),
            PhysicsColliderShapeConfig::Capsule { .. } | PhysicsColliderShapeConfig::Mesh => {
                anyhow::bail!(
                    "Unsupported physics collider on object '{name}': T6 supports Box and Sphere"
                )
            }
        }
    }
    Ok(())
}
