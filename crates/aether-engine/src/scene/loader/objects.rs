//! Object spawning helpers for `SceneLoader`.

use crate::{
    asset::{mesh::GpuMesh, registry::BuiltinMeshRegistry},
    ecs::components::{MeshHandle, Name, Transform, Visibility},
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
    world: &mut World,
) -> anyhow::Result<()> {
    let mut mesh_cache: HashMap<String, Arc<GpuMesh>> = HashMap::new();

    for obj in &desc.objects {
        let mesh_name = match &obj.mesh {
            MeshRef::Builtin(name) => name.clone(),
            MeshRef::File(path) => {
                anyhow::bail!(
                    "File mesh not yet supported: '{}' (Phase 1 limitation)",
                    path
                );
            }
        };

        if !mesh_cache.contains_key(&mesh_name) {
            let cpu_mesh = registry
                .get(&mesh_name)
                .ok_or_else(|| anyhow::anyhow!("Unknown built-in mesh: '{}'", mesh_name))?;
            mesh_cache.insert(
                mesh_name.clone(),
                Arc::new(GpuMesh::from_cpu(device, &cpu_mesh)),
            );
        }
        let gpu_mesh = mesh_cache.get(&mesh_name).unwrap();

        let transform = Transform {
            translation: Vec3::from_array(obj.transform.translation),
            rotation: Quat::from_array(obj.transform.rotation),
            scale: Vec3::from_array(obj.transform.scale),
        };

        let material = MaterialUniform {
            albedo: obj.material.albedo,
            roughness: obj.material.roughness,
            metallic: obj.material.metallic,
            _pad: [0.0, 0.0],
        };

        world.spawn((
            transform,
            MeshHandle::new(gpu_mesh.clone(), mesh_name),
            material,
            Visibility::default(),
            Name(obj.name.clone()),
        ));
    }
    Ok(())
}
