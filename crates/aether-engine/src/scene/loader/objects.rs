//! Object spawning helpers for `SceneLoader`.

use crate::{
    asset::{
        mesh::{CpuMesh, GpuMesh},
        registry::BuiltinMeshRegistry,
        AssetManager,
    },
    ecs::components::{MeshHandle, MeshSource, Name, Transform, Visibility},
    ecs::World,
    renderer::renderable::MaterialUniform,
    scene::{material::MaterialResolver, MaterialConfig, MeshRef, SceneDescription},
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

                    world.spawn((
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
                }
                continue;
            }
        }

        let resolution = material_resolver.resolve(&obj.material, assets)?;
        let material = MaterialUniform::from_resolution(&resolution);

        world.spawn((
            transform,
            MeshHandle::new(base_gpu_mesh, mesh_source, mesh_name),
            obj.material.clone(),
            material,
            Visibility(obj.visible),
            Name(obj.name.clone()),
        ));
    }
    Ok(())
}
