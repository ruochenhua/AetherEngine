//! Object spawning helpers for `SceneLoader`.

use crate::{
    asset::{
        mesh::{CpuMesh, GpuMesh},
        registry::BuiltinMeshRegistry,
        texture::CpuTexture,
        AssetManager,
    },
    ecs::components::{MeshHandle, MeshSource, Name, Transform, Visibility},
    ecs::World,
    renderer::renderable::MaterialUniform,
    scene::{MeshRef, SceneDescription},
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

                    let albedo_texture_id = match &submesh.material.albedo_texture {
                        Some(path) => match assets.load::<CpuTexture>(path) {
                            Ok(handle) => handle.id(),
                            Err(e) => {
                                tracing::warn!("Failed to load albedo texture '{}': {}", path, e);
                                0
                            }
                        },
                        None => 0,
                    };

                    let material = MaterialUniform {
                        albedo: submesh.material.base_color,
                        roughness: submesh.material.roughness,
                        metallic: submesh.material.metallic,
                        _pad: [0.0, 0.0],
                        albedo_texture_id,
                    };

                    world.spawn((
                        transform.clone(),
                        MeshHandle::new(
                            gpu_mesh,
                            mesh_source.clone(),
                            format!("{}::{}", mesh_name, submesh.name),
                        ),
                        material,
                        Visibility::default(),
                        Name(format!("{}::{}", obj.name, submesh.name)),
                    ));
                }
                continue;
            }
        }

        let albedo_texture_id = match &obj.material.albedo_texture {
            Some(path) => match assets.load::<CpuTexture>(path) {
                Ok(handle) => handle.id(),
                Err(e) => {
                    tracing::warn!("Failed to load albedo texture '{}': {}", path, e);
                    0
                }
            },
            None => 0,
        };

        let material = MaterialUniform {
            albedo: obj.material.albedo,
            roughness: obj.material.roughness,
            metallic: obj.material.metallic,
            _pad: [0.0, 0.0],
            albedo_texture_id,
        };

        world.spawn((
            transform,
            MeshHandle::new(base_gpu_mesh, mesh_source, mesh_name),
            material,
            Visibility::default(),
            Name(obj.name.clone()),
        ));
    }
    Ok(())
}
