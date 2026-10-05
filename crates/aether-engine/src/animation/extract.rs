use super::palette::{build_joint_palette, skinned_world_bounds};
use super::types::{
    AnimationContext, AnimationDiagnostic, AnimationError, AnimationFrame, AnimationItemResult,
    AnimationPrimitiveMapping, AnimatorRuntime, JointPaletteGpu, StaticFallbackReason,
};
use crate::asset::id::AssetId;
use crate::asset::loaders::gltf::{GltfDocumentAsset, SkeletonAsset};
use crate::math::Mat4;
use std::sync::Arc;

/// Resolve owned animation inputs from the asset snapshot pinned to this frame.
pub trait AnimationAssets {
    /// Resolve one immutable source glTF document.
    fn document(&self, id: &AssetId) -> Option<Arc<GltfDocumentAsset>>;
    /// Resolve one immutable derived skeleton.
    fn skeleton(&self, id: &AssetId) -> Option<Arc<SkeletonAsset>>;
}

/// Extract an immutable frame; stale scene contexts reject the whole publication.
pub fn extract_animation_frame(
    runtime: &mut AnimatorRuntime,
    context: &AnimationContext<'_>,
) -> Result<AnimationFrame, AnimationError> {
    if runtime.generation != context.scene_generation
        || context.snapshot.generation != context.scene_generation
    {
        return Err(AnimationError::StaleGeneration {
            runtime: runtime.generation,
            snapshot: context.snapshot.generation,
            requested: context.scene_generation,
        });
    }
    if let Some(last) = runtime.last_published_frame {
        if context.frame_id <= last {
            return Err(AnimationError::StaleFrame {
                last,
                requested: context.frame_id,
            });
        }
    }

    let mut items = Vec::with_capacity(context.primitives.primitives.len());
    let mut diagnostics = Vec::new();
    for mapping in context.primitives.primitives.iter() {
        let document = resolve_document(context, mapping)?;
        let primitive = document
            .meshes
            .get(mapping.mesh_index as usize)
            .and_then(|mesh| mesh.primitives.get(mapping.primitive_index as usize))
            .ok_or(AnimationError::InvalidPrimitiveMapping {
                entity_id: mapping.entity_id,
                mesh_index: mapping.mesh_index,
                primitive_index: mapping.primitive_index,
            })?;

        let result = if mapping.joint_count > 128 {
            fallback(mapping, StaticFallbackReason::JointLimit, &mut diagnostics)
        } else if primitive.joints.is_none() || primitive.weights.is_none() {
            fallback(
                mapping,
                StaticFallbackReason::MissingSkinStreams,
                &mut diagnostics,
            )
        } else {
            let skeleton = resolve_skeleton(context, mapping)?;
            let pose = runtime.poses.get(&mapping.entity_id);
            match pose {
                None => fallback(mapping, StaticFallbackReason::MissingPose, &mut diagnostics),
                Some(pose) => {
                    let palette = match build_joint_palette(&document, &skeleton, pose) {
                        Ok(palette) => Ok(palette),
                        Err(AnimationError::InvalidPaletteSize(_)) => {
                            Err(StaticFallbackReason::JointLimit)
                        }
                        Err(AnimationError::PoseJointCountMismatch { .. }) => {
                            Err(StaticFallbackReason::MissingPose)
                        }
                        Err(AnimationError::NonFiniteMatrix) => {
                            Err(StaticFallbackReason::InvalidNumericData)
                        }
                        Err(error) => return Err(error),
                    };
                    match palette {
                        Err(reason) => fallback(mapping, reason, &mut diagnostics),
                        Ok(palette) => match JointPaletteGpu::from_palette(&palette) {
                            Err(_) => fallback(
                                mapping,
                                StaticFallbackReason::InvalidNumericData,
                                &mut diagnostics,
                            ),
                            Ok(gpu_palette) => {
                                let world = Mat4::from_cols_array_2d(&mapping.world_transform);
                                match skinned_world_bounds(primitive, &palette, world) {
                                    Ok(world_bounds) => AnimationItemResult::Skinned {
                                        palette: Arc::new(gpu_palette),
                                        world_bounds,
                                    },
                                    Err(reason) => fallback(mapping, reason, &mut diagnostics),
                                }
                            }
                        },
                    }
                }
            }
        };
        items.push(super::types::AnimationRenderItem {
            entity_id: mapping.entity_id,
            primitive_index: mapping.primitive_index,
            result,
        });
    }

    runtime.last_published_frame = Some(context.frame_id);
    Ok(AnimationFrame {
        scene_generation: context.scene_generation,
        frame_id: context.frame_id,
        items: Arc::from(items),
        diagnostics: Arc::from(diagnostics),
    })
}

fn resolve_document(
    context: &AnimationContext<'_>,
    mapping: &AnimationPrimitiveMapping,
) -> Result<Arc<GltfDocumentAsset>, AnimationError> {
    context
        .assets
        .document(&mapping.document)
        .ok_or_else(|| AnimationError::MissingAsset {
            kind: "gltf_document",
            id: mapping.document.to_string(),
        })
}

fn resolve_skeleton(
    context: &AnimationContext<'_>,
    mapping: &AnimationPrimitiveMapping,
) -> Result<Arc<SkeletonAsset>, AnimationError> {
    let id = mapping
        .skeleton
        .as_ref()
        .ok_or_else(|| AnimationError::MissingAsset {
            kind: "skeleton",
            id: "<unassigned>".to_owned(),
        })?;
    context
        .assets
        .skeleton(id)
        .ok_or_else(|| AnimationError::MissingAsset {
            kind: "skeleton",
            id: id.to_string(),
        })
}

fn fallback(
    mapping: &AnimationPrimitiveMapping,
    reason: StaticFallbackReason,
    diagnostics: &mut Vec<AnimationDiagnostic>,
) -> AnimationItemResult {
    diagnostics.push(AnimationDiagnostic {
        entity_id: mapping.entity_id,
        primitive_index: mapping.primitive_index,
        reason: reason.clone(),
    });
    AnimationItemResult::StaticFallback(reason)
}
