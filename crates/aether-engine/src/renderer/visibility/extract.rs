//! ECS visibility extraction and stable mesh/material identity.

use super::{
    evaluate_visibility, BatchKey, LodHistory, VisibilityCandidate, VisibilityFrame,
    VisibilityPolicy,
};
use crate::ecs::components::{MeshHandle, MeshSource, Transform, Visibility};
use crate::ecs::World;
use crate::math::{Frustum, Mat4};
use crate::renderer::camera::FlyCamera;
use crate::renderer::extracted_material::ExtractedMaterial;
use crate::renderer::renderable::MaterialUniform;
use crate::renderer::transparent::{TransparentBlendMode, TransparentMaterial};
use sha2::{Digest, Sha256};

/// Extract visibility candidates from renderable ECS entities and apply policy.
pub fn extract_visibility(
    world: &World,
    camera: &FlyCamera,
    viewport_width: u32,
    viewport_height: u32,
    policy: VisibilityPolicy,
    history: &mut LodHistory,
) -> VisibilityFrame {
    extract_visibility_with_material_mode(
        world,
        camera,
        viewport_width,
        viewport_height,
        policy,
        history,
        false,
    )
}

/// Extract visibility candidates through the canonical material component when enabled.
pub fn extract_visibility_with_material_mode(
    world: &World,
    camera: &FlyCamera,
    viewport_width: u32,
    viewport_height: u32,
    policy: VisibilityPolicy,
    history: &mut LodHistory,
    extracted_material_enabled: bool,
) -> VisibilityFrame {
    let aspect = if viewport_height == 0 {
        1.0
    } else {
        viewport_width.max(1) as f32 / viewport_height as f32
    };
    let view_projection = camera.projection_matrix(aspect) * camera.view_matrix();
    let frustum = Frustum::from_view_projection(view_projection);
    let candidates = if extracted_material_enabled {
        world
            .query::<(
                hecs::Entity,
                &Transform,
                &MeshHandle,
                &ExtractedMaterial,
                &Visibility,
                Option<&TransparentMaterial>,
            )>()
            .iter()
            .map(
                |(entity, transform, mesh, material, visibility, transparent)| {
                    let model = Mat4::from_scale_rotation_translation(
                        transform.scale,
                        transform.rotation,
                        transform.translation,
                    );
                    VisibilityCandidate {
                        entity_bits: entity.to_bits().get(),
                        batch_key: make_batch_key(mesh, material, transparent),
                        bounds: mesh.mesh.aabb.transform(model),
                        enabled: visibility.0,
                    }
                },
            )
            .collect::<Vec<_>>()
    } else {
        world
            .query::<(
                hecs::Entity,
                &Transform,
                &MeshHandle,
                &MaterialUniform,
                &Visibility,
                Option<&TransparentMaterial>,
            )>()
            .iter()
            .map(
                |(entity, transform, mesh, material, visibility, transparent)| {
                    let model = Mat4::from_scale_rotation_translation(
                        transform.scale,
                        transform.rotation,
                        transform.translation,
                    );
                    let extracted = ExtractedMaterial::from_legacy_uniform(*material);
                    VisibilityCandidate {
                        entity_bits: entity.to_bits().get(),
                        batch_key: make_batch_key(mesh, &extracted, transparent),
                        bounds: mesh.mesh.aabb.transform(model),
                        enabled: visibility.0,
                    }
                },
            )
            .collect::<Vec<_>>()
    };
    evaluate_visibility(
        candidates,
        view_projection,
        &frustum,
        viewport_height,
        policy,
        history,
    )
}

fn make_batch_key(
    mesh: &MeshHandle,
    material: &ExtractedMaterial,
    transparent: Option<&TransparentMaterial>,
) -> BatchKey {
    let mesh_source = match &mesh.source {
        MeshSource::Builtin(name) => format!("builtin:{name}"),
        MeshSource::File(path) => format!("file:{}", path.replace('\\', "/")),
    };
    let (transparent_blend, alpha_cutoff_bits) = transparent.map_or((None, None), |value| {
        let blend = match value.blend {
            TransparentBlendMode::Alpha => 1,
            TransparentBlendMode::Additive => 2,
        };
        (Some(blend), value.alpha_cutoff.map(f32::to_bits))
    });
    let mut material_hash = Sha256::new();
    material_hash.update(bytemuck::bytes_of(material.uniform_adapter()));
    material_hash.update(material.orm_swizzle_bits());
    BatchKey {
        mesh_source,
        material_sha256: material_hash.finalize().into(),
        transparent_blend,
        alpha_cutoff_bits,
    }
}
