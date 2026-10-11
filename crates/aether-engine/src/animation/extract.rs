//! Per-frame animation extraction; renderer-facing data is immutable.

use super::palette::{build_joint_palette, JointPalette};
use super::player::{sample_animator_with_looping, AnimationError, AnimatorRuntime};
use super::validate_skinning_inputs;
use crate::asset::gltf_document::{AnimationClipAsset, GltfDocument, SkeletonAsset};
use crate::asset::store::AssetId;
use crate::asset::{mesh::CpuMesh, AssetManager, Handle};
use crate::math::{Aabb, Mat4};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// Persistent mesh identity and version used by an animation render item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshRef {
    /// Asset key.
    pub asset: AssetId,
    /// Mesh generation captured during extraction.
    pub generation: u32,
}

/// Immutable scene metadata captured at the animation boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneSnapshot {
    /// Scene content generation.
    pub generation: u64,
    /// Frame boundary represented by this snapshot.
    pub frame_id: u64,
}

/// Minimal camera data used to keep extraction context explicit.
#[derive(Debug, Clone, Copy)]
pub struct AnimationCameraFrame {
    /// View-projection matrix for visibility-aware animation consumers.
    pub view_projection: Mat4,
}

/// A mesh primitive that may be animated.
#[derive(Debug, Clone)]
pub struct AnimationPrimitive {
    /// Stable entity bits from the scene snapshot.
    pub entity_bits: u64,
    /// Index in the source glTF mesh.
    pub primitive_index: u32,
    /// Source node that references the mesh/skin pair.
    pub node_index: usize,
    /// Mesh asset and generation.
    pub mesh: MeshRef,
    /// Owned source document for bind nodes and primitive data.
    pub document: Handle<GltfDocument>,
    /// Skeleton selected for this primitive.
    pub skeleton: Option<Handle<SkeletonAsset>>,
    /// Static geometry used for fallback and conservative bounds.
    pub static_mesh: Arc<CpuMesh>,
    /// World transform for the entity.
    pub world_transform: Mat4,
}

/// Resolve immutable animation assets at extraction time.
pub trait AnimationAssets {
    /// Resolve an owned glTF source document.
    fn document(&self, handle: Handle<GltfDocument>) -> Result<Arc<GltfDocument>, AnimationError>;
    /// Resolve a typed skeleton.
    fn skeleton(&self, handle: Handle<SkeletonAsset>)
        -> Result<Arc<SkeletonAsset>, AnimationError>;
    /// Resolve a typed clip.
    fn clip(
        &self,
        handle: Handle<AnimationClipAsset>,
    ) -> Result<Arc<AnimationClipAsset>, AnimationError>;
}

impl AnimationAssets for AssetManager {
    fn document(&self, handle: Handle<GltfDocument>) -> Result<Arc<GltfDocument>, AnimationError> {
        self.get(handle).ok_or(AnimationError::MissingAsset)
    }

    fn skeleton(
        &self,
        handle: Handle<SkeletonAsset>,
    ) -> Result<Arc<SkeletonAsset>, AnimationError> {
        self.get(handle).ok_or(AnimationError::MissingAsset)
    }

    fn clip(
        &self,
        handle: Handle<AnimationClipAsset>,
    ) -> Result<Arc<AnimationClipAsset>, AnimationError> {
        self.get(handle).ok_or(AnimationError::MissingAsset)
    }
}

/// Immutable inputs for one animation extraction boundary.
pub struct AnimationContext<'a> {
    /// Scene identity and frame marker.
    pub snapshot: &'a SceneSnapshot,
    /// Typed asset resolver.
    pub assets: &'a dyn AnimationAssets,
    /// Per-entity/per-primitive mapping.
    pub primitives: &'a [AnimationPrimitive],
    /// Camera frame captured with the scene.
    pub camera: &'a AnimationCameraFrame,
    /// Scene generation expected by the caller.
    pub scene_generation: u64,
    /// Render frame id shared by published data.
    pub frame_id: u64,
}

/// One animation diagnostic associated with a source primitive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationDiagnostic {
    /// Entity that produced the finding.
    pub entity_bits: u64,
    /// Primitive that produced the finding.
    pub primitive_index: u32,
    /// Stable diagnostic code.
    pub code: String,
}

/// A successful skinning item with one shared palette and bounds value.
#[derive(Debug, Clone)]
pub struct SkinnedRenderItem {
    /// Entity owning this primitive.
    pub entity_bits: u64,
    /// Source mesh primitive index.
    pub primitive_index: u32,
    /// Persistent mesh reference.
    pub mesh: MeshRef,
    /// Typed skeleton handle.
    pub skeleton: Handle<SkeletonAsset>,
    /// Immutable joint palette.
    pub palette: Arc<JointPalette>,
    /// Original source geometry whose joints and weights drive GPU skinning.
    pub source_mesh: Arc<CpuMesh>,
    /// Four validated joint influences for every source vertex.
    pub weights: Arc<[crate::asset::gltf_document::JointWeights]>,
    /// Animator time used to produce this palette.
    pub sample_time: f32,
    /// World transform.
    pub world_transform: Mat4,
    /// Shared conservative bounds sent to every render pass.
    pub bounds: Aabb,
}

/// Static rendering fallback for one affected primitive.
#[derive(Debug, Clone)]
pub struct StaticFallback {
    /// Entity owning this primitive.
    pub entity_bits: u64,
    /// Source mesh primitive index.
    pub primitive_index: u32,
    /// Persistent mesh reference.
    pub mesh: MeshRef,
    /// World transform.
    pub world_transform: Mat4,
    /// Static mesh bounds in world space.
    pub bounds: Aabb,
    /// Local reason this primitive did not produce a skin palette.
    pub reason: AnimationError,
}

/// Renderer-facing item; parser and mutable animator state are not exposed.
#[derive(Debug, Clone)]
pub enum AnimationRenderItem {
    /// Palette-backed item.
    Skinned(SkinnedRenderItem),
    /// Per-primitive static fallback.
    StaticFallback(StaticFallback),
}

/// Immutable frame published to GBuffer, shadow, and water-reflection consumers.
#[derive(Debug, Clone)]
pub struct AnimationFrame {
    /// Frame boundary shared with render extraction.
    pub frame_id: u64,
    /// Scene generation that produced this frame.
    pub scene_generation: u64,
    /// Immutable render items.
    pub items: Arc<[AnimationRenderItem]>,
    /// Per-item diagnostics.
    pub diagnostics: Arc<[AnimationDiagnostic]>,
    /// Deterministic state digest for replay and visual cases.
    pub state_hash: [u8; 32],
}

impl AnimationFrame {
    /// Conservative world-space bounds published to geometry consumers.
    pub fn conservative_bounds(&self) -> Arc<[Aabb]> {
        Arc::from(
            self.items
                .iter()
                .map(|item| match item {
                    AnimationRenderItem::Skinned(item) => item.bounds,
                    AnimationRenderItem::StaticFallback(item) => item.bounds,
                })
                .collect::<Vec<_>>(),
        )
    }
}

/// Sample active animators and publish per-primitive immutable render data.
pub fn extract_animation_frame(
    runtime: &AnimatorRuntime,
    context: &AnimationContext<'_>,
) -> Result<AnimationFrame, AnimationError> {
    if runtime.generation != context.scene_generation {
        return Err(AnimationError::StaleGeneration {
            expected: context.scene_generation,
            actual: runtime.generation,
        });
    }
    if context.snapshot.generation != context.scene_generation
        || context.snapshot.frame_id != context.frame_id
        || context.frame_id == 0
    {
        return Err(AnimationError::StaleGeneration {
            expected: context.scene_generation,
            actual: context.snapshot.generation,
        });
    }
    let _view_projection = context.camera.view_projection;
    let mut items = Vec::with_capacity(context.primitives.len());
    let mut diagnostics = Vec::new();
    let mut hash_state = String::new();
    for primitive in context.primitives {
        let bind_bounds = if primitive.static_mesh.positions.is_empty() {
            Aabb::new(glam::Vec3::ZERO, glam::Vec3::ZERO)
        } else {
            primitive.static_mesh.compute_aabb()
        };
        let fallback = |reason: AnimationError| {
            AnimationRenderItem::StaticFallback(StaticFallback {
                entity_bits: primitive.entity_bits,
                primitive_index: primitive.primitive_index,
                mesh: primitive.mesh.clone(),
                world_transform: primitive.world_transform,
                bounds: bind_bounds.transform(primitive.world_transform),
                reason,
            })
        };
        let document = match context.assets.document(primitive.document.clone()) {
            Ok(document) => document,
            Err(error) => {
                diagnostics.push(diagnostic(primitive, &error));
                items.push(fallback(error));
                continue;
            }
        };
        let node = match document.nodes.get(primitive.node_index) {
            Some(node) => node,
            None => {
                let error = AnimationError::MissingJoint(primitive.node_index);
                diagnostics.push(diagnostic(primitive, &error));
                items.push(fallback(error));
                continue;
            }
        };
        if let Some(skin_index) = node.skin {
            if let Some(skin) = document.skins.get(skin_index) {
                if skin.joints.len() > crate::asset::gltf_document::MAX_SKELETON_JOINTS {
                    let error = AnimationError::Document(
                        crate::asset::gltf_document::ValidationError::UnsupportedJointCount {
                            count: skin.joints.len(),
                        },
                    );
                    diagnostics.push(diagnostic(primitive, &error));
                    items.push(fallback(error));
                    continue;
                }
            }
        } else {
            continue;
        }
        let Some(skeleton_handle) = primitive.skeleton.clone() else {
            let error = AnimationError::MissingAsset;
            diagnostics.push(diagnostic(primitive, &error));
            items.push(fallback(error));
            continue;
        };
        let Some(state) = runtime.states.get(&primitive.entity_bits) else {
            let error = AnimationError::MissingState(primitive.entity_bits);
            diagnostics.push(diagnostic(primitive, &error));
            items.push(fallback(error));
            continue;
        };
        let resolved = (|| {
            let skeleton = context.assets.skeleton(skeleton_handle.clone())?;
            let clip = match &state.clip {
                Some(handle) => Some(context.assets.clip(handle.clone())?),
                None => None,
            };
            let pose = sample_animator_with_looping(
                &document,
                &skeleton,
                clip.as_deref(),
                state.time,
                state.looping,
            )?;
            if pose.bind_pose_fallback {
                return Err(AnimationError::MissingClip);
            }
            let palette = build_joint_palette(
                &document,
                &skeleton,
                &pose,
                bind_bounds,
                primitive.world_transform,
                context.scene_generation,
            )?;
            let mesh_index = node.mesh.ok_or(AnimationError::InvalidPalette)?;
            let document_primitive = document
                .meshes
                .get(mesh_index)
                .and_then(|mesh| mesh.primitives.get(primitive.primitive_index as usize))
                .ok_or(AnimationError::InvalidPalette)?;
            let source_mesh = document_primitive
                .mesh
                .as_ref()
                .ok_or(AnimationError::InvalidPalette)?;
            validate_skinning_inputs(
                source_mesh.positions.len(),
                &document_primitive.weights,
                &palette,
            )?;
            let clip_duration = clip.as_ref().map_or(0.0, |clip| clip.duration);
            Ok::<_, AnimationError>((
                skeleton,
                pose,
                palette,
                Arc::new(source_mesh.clone()),
                Arc::from(document_primitive.weights.clone()),
                clip_duration,
            ))
        })();
        match resolved {
            Ok((_skeleton, pose, palette, source_mesh, weights, clip_duration)) => {
                let local = pose
                    .locals
                    .iter()
                    .map(|p| {
                        format!(
                            "{:.9},{:.9},{:.9}|{:.9},{:.9},{:.9},{:.9}|{:.9},{:.9},{:.9}",
                            p.translation.x,
                            p.translation.y,
                            p.translation.z,
                            p.rotation.x,
                            p.rotation.y,
                            p.rotation.z,
                            p.rotation.w,
                            p.scale.x,
                            p.scale.y,
                            p.scale.z
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(";");
                hash_state.push_str(&format!(
                    "{}:{}:{};{}\n",
                    primitive.entity_bits, primitive.primitive_index, state.time, local
                ));
                items.push(AnimationRenderItem::Skinned(SkinnedRenderItem {
                    entity_bits: primitive.entity_bits,
                    primitive_index: primitive.primitive_index,
                    mesh: primitive.mesh.clone(),
                    skeleton: skeleton_handle,
                    bounds: palette.bounds,
                    palette,
                    source_mesh,
                    weights,
                    sample_time: if state.looping {
                        state.time.rem_euclid(clip_duration)
                    } else {
                        state.time.clamp(0.0, clip_duration)
                    },
                    world_transform: primitive.world_transform,
                }));
            }
            Err(error) => {
                diagnostics.push(diagnostic(primitive, &error));
                items.push(fallback(error));
            }
        }
    }
    let state_hash = Sha256::digest(hash_state.as_bytes()).into();
    Ok(AnimationFrame {
        frame_id: context.frame_id,
        scene_generation: context.scene_generation,
        items: Arc::from(items),
        diagnostics: Arc::from(diagnostics),
        state_hash,
    })
}

fn diagnostic(primitive: &AnimationPrimitive, error: &AnimationError) -> AnimationDiagnostic {
    AnimationDiagnostic {
        entity_bits: primitive.entity_bits,
        primitive_index: primitive.primitive_index,
        code: format!("{error:?}"),
    }
}

#[cfg(test)]
#[path = "extract_tests.rs"]
mod tests;
