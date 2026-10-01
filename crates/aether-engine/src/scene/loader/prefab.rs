//! Scene integration for Prefab assets.

use crate::asset::{
    mesh::CpuMesh, mesh::GpuMesh, prefab::PrefabAsset, prefab::PrefabContext,
    registry::BuiltinMeshRegistry, AssetManager,
};
use crate::ecs::components::{MeshHandle, MeshSource};
use crate::ecs::components::{PrefabInstanceRoot, PrefabNodeInstance};
use crate::ecs::World;
use crate::renderer::renderable::MaterialUniform;
use crate::renderer::transparent::TransparentMaterial;
use crate::scene::material::MaterialResolver;
use crate::scene::{MaterialConfig, MeshRef, SceneDescription};
use crate::{asset::prefab, asset::prefab::PrefabAssets};
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

pub(super) fn replace_instances(
    prefab_path: &str,
    device: &wgpu::Device,
    registry: &BuiltinMeshRegistry,
    assets: &mut AssetManager,
    world: &mut World,
) -> anyhow::Result<usize> {
    let handle = assets.load::<PrefabAsset>(prefab_path)?;
    let document = assets
        .get(handle)
        .ok_or_else(|| anyhow::anyhow!("prefab asset '{prefab_path}' is not ready"))?
        .document
        .clone();
    let instances = world
        .query::<&PrefabInstanceRoot>()
        .iter()
        .filter_map(|root| (root.0.prefab_asset == prefab_path).then_some(root.0.clone()))
        .collect::<Vec<_>>();
    let mut replaced = 0;

    for instance in instances {
        let staging_id = unused_instance_id(world)?;
        let mut staging = instance.clone();
        staging.instance_id = staging_id;
        let staged_nodes = {
            let mut resolver = ScenePrefabAssets {
                device,
                registry,
                assets,
                meshes: HashMap::new(),
            };
            let mut context = PrefabContext::new(&staging, &mut resolver);
            prefab::instantiate(&document, &mut context, world)?
        };

        let old_entities = world
            .query::<(crate::ecs::Entity, &PrefabNodeInstance)>()
            .iter()
            .filter_map(|(entity, marker)| {
                (marker.prefab_instance_id == instance.instance_id).then_some(entity)
            })
            .collect::<Vec<_>>();
        for entity in old_entities {
            world.despawn(entity)?;
        }
        for (node_id, entity) in staged_nodes {
            let parent_instance_id = world
                .query_one::<&PrefabNodeInstance>(entity)
                .get()
                .map_err(|error| anyhow::anyhow!("staged prefab node disappeared: {error}"))?
                .parent_instance_id;
            world.insert(
                entity,
                (PrefabNodeInstance {
                    prefab_instance_id: instance.instance_id,
                    instance_id: node_id,
                    parent_instance_id,
                },),
            )?;
            if node_id == document.root.instance_id {
                world.insert(entity, (PrefabInstanceRoot(instance.clone()),))?;
            }
        }
        replaced += 1;
    }
    Ok(replaced)
}

fn unused_instance_id(world: &World) -> anyhow::Result<u64> {
    let occupied = world
        .query::<&PrefabNodeInstance>()
        .iter()
        .map(|node| node.prefab_instance_id)
        .collect::<std::collections::HashSet<_>>();
    (0..u64::MAX)
        .rev()
        .find(|id| !occupied.contains(id))
        .ok_or_else(|| anyhow::anyhow!("no temporary prefab instance id is available"))
}

pub(super) fn build_prefab_instances(
    desc: &SceneDescription,
    device: &wgpu::Device,
    registry: &BuiltinMeshRegistry,
    assets: &mut AssetManager,
    world: &mut World,
) -> anyhow::Result<()> {
    let mut seen_ids = BTreeSet::new();
    let mut loaded = Vec::with_capacity(desc.prefab_instances.len());
    for instance in &desc.prefab_instances {
        anyhow::ensure!(
            seen_ids.insert(instance.instance_id),
            "duplicate prefab instance id {}",
            instance.instance_id
        );
        let handle = assets
            .load::<PrefabAsset>(&instance.prefab_asset)
            .map_err(|error| {
                anyhow::anyhow!(
                    "failed to load prefab '{}' for instance {}: {error}",
                    instance.prefab_asset,
                    instance.instance_id
                )
            })?;
        let prefab = assets.get(handle).ok_or_else(|| {
            anyhow::anyhow!("prefab asset '{}' was not ready", instance.prefab_asset)
        })?;
        loaded.push((instance.clone(), prefab.document.clone()));
    }

    let mut resolver = ScenePrefabAssets {
        device,
        registry,
        assets,
        meshes: HashMap::new(),
    };
    for (instance, document) in loaded {
        let mut context = PrefabContext::new(&instance, &mut resolver);
        prefab::instantiate(&document, &mut context, world).map_err(|error| {
            anyhow::anyhow!("prefab instance {}: {error}", instance.instance_id)
        })?;
    }
    Ok(())
}

struct ScenePrefabAssets<'a> {
    device: &'a wgpu::Device,
    registry: &'a BuiltinMeshRegistry,
    assets: &'a mut AssetManager,
    meshes: HashMap<String, Arc<GpuMesh>>,
}

impl PrefabAssets for ScenePrefabAssets<'_> {
    fn resolve_mesh(&mut self, source: &MeshRef) -> Result<MeshHandle, prefab::PrefabError> {
        let (key, mesh_source, cpu_mesh) = match source {
            MeshRef::Builtin(name) => {
                let cpu = self.registry.get(name).ok_or_else(|| {
                    prefab::PrefabError::Dependency(format!("unknown built-in mesh '{name}'"))
                })?;
                (
                    format!("builtin:{name}"),
                    MeshSource::Builtin(name.clone()),
                    Some(Arc::new(cpu)),
                )
            }
            MeshRef::File(path) => {
                let handle = self.assets.load::<CpuMesh>(path).map_err(|error| {
                    prefab::PrefabError::Dependency(format!("mesh '{path}': {error}"))
                })?;
                let cpu = self.assets.get(handle).ok_or_else(|| {
                    prefab::PrefabError::Dependency(format!("mesh '{path}' was not ready"))
                })?;
                if !cpu.submeshes.is_empty() {
                    return Err(prefab::PrefabError::UnsupportedComponent(format!(
                        "mesh '{path}' has multiple material submeshes"
                    )));
                }
                (
                    format!("file:{path}"),
                    MeshSource::File(path.clone()),
                    Some(cpu),
                )
            }
        };
        if !self.meshes.contains_key(&key) {
            let cpu = cpu_mesh.ok_or_else(|| {
                prefab::PrefabError::Dependency(format!("mesh source '{key}' is unavailable"))
            })?;
            self.meshes
                .insert(key.clone(), Arc::new(GpuMesh::from_cpu(self.device, &cpu)));
        }
        let mesh = self.meshes.get(&key).cloned().ok_or_else(|| {
            prefab::PrefabError::Dependency(format!("mesh cache entry '{key}' is missing"))
        })?;
        Ok(MeshHandle::new(mesh, mesh_source, key))
    }

    fn resolve_material(
        &mut self,
        config: &MaterialConfig,
    ) -> Result<prefab::ResolvedPrefabMaterial, prefab::PrefabError> {
        let mut resolved_config = config.clone();
        if let Some(texture) = config
            .transparent
            .as_ref()
            .and_then(|transparent| transparent.texture.as_ref())
        {
            resolved_config.albedo_texture = Some(texture.clone());
        }
        let project_root = self.assets.project_root().to_path_buf();
        let resolution = MaterialResolver::new(&project_root)
            .resolve(&resolved_config, self.assets)
            .map_err(|error| prefab::PrefabError::Dependency(error.to_string()))?;
        let transparent = config
            .transparent
            .as_ref()
            .map(|settings| {
                let material = TransparentMaterial {
                    base_color: config.albedo,
                    texture: resolution.material.albedo,
                    blend: settings.blend,
                    alpha_cutoff: settings.alpha_cutoff,
                };
                material
                    .validate()
                    .map_err(|error| prefab::PrefabError::Dependency(format!("{error:?}")))?;
                Ok(material)
            })
            .transpose()?;
        Ok(prefab::ResolvedPrefabMaterial {
            uniform: MaterialUniform::from_resolution(&resolution),
            transparent,
        })
    }
}
