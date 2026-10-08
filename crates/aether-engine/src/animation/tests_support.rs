use super::{AnimationFrame, AnimationRenderItem, MeshRef, StaticFallback};
use crate::animation::AnimationError;
use crate::asset::store::{canonicalize, AssetId, AssetKind};
use crate::math::Aabb;
use std::path::Path;
use std::sync::Arc;

pub(crate) fn animation_frame_with_bounds(bounds: Aabb) -> AnimationFrame {
    AnimationFrame {
        frame_id: 1,
        scene_generation: 1,
        items: Arc::from([AnimationRenderItem::StaticFallback(StaticFallback {
            entity_bits: 1,
            primitive_index: 0,
            mesh: MeshRef {
                asset: AssetId {
                    kind: AssetKind::CpuMesh,
                    path: canonicalize(Path::new("."), Path::new("tests/fixtures/mesh.gltf"))
                        .expect("fixture asset id is canonical"),
                },
                generation: 1,
            },
            world_transform: crate::math::Mat4::IDENTITY,
            bounds,
            reason: AnimationError::MissingAsset,
        })]),
        diagnostics: Arc::from([]),
        state_hash: [0; 32],
    }
}
