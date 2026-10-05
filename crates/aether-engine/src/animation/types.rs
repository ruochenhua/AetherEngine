use crate::asset::id::AssetId;
use crate::math::{Aabb, Mat4};
use std::collections::BTreeMap;
use std::sync::Arc;
use thiserror::Error;

/// One sampled local transform for a palette joint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalPose {
    /// Translation in the parent joint's local space.
    pub translation: [f32; 3],
    /// Quaternion in `[x, y, z, w]` order.
    pub rotation: [f32; 4],
    /// Scale in the parent joint's local space.
    pub scale: [f32; 3],
}

impl LocalPose {
    /// Return the bind pose from a glTF joint.
    pub fn from_joint(joint: &crate::asset::loaders::gltf::Joint) -> Self {
        Self {
            translation: joint.translation,
            rotation: joint.rotation,
            scale: joint.scale,
        }
    }

    pub(crate) fn matrix(self) -> Option<Mat4> {
        if self
            .translation
            .iter()
            .chain(self.rotation.iter())
            .chain(self.scale.iter())
            .any(|value| !value.is_finite())
        {
            return None;
        }
        let rotation = glam::Quat::from_array(self.rotation);
        if rotation.length_squared() <= 1.0e-12 {
            return None;
        }
        let matrix = Mat4::from_scale_rotation_translation(
            glam::Vec3::from_array(self.scale),
            rotation.normalize(),
            glam::Vec3::from_array(self.translation),
        );
        matrix_is_finite(matrix).then_some(matrix)
    }
}

/// Immutable scene identity captured before animation extraction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SceneSnapshot {
    /// Active scene generation at the frame boundary.
    pub generation: u64,
}

/// Camera state captured for one extraction boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationCameraFrame {
    /// Camera view-projection matrix in column-major order.
    pub view_projection: [[f32; 4]; 4],
}

/// Mapping from one entity primitive to its immutable glTF and skeleton assets.
#[derive(Clone, Debug)]
pub struct AnimationPrimitiveMapping {
    /// Stable entity bits used in diagnostics and render extraction.
    pub entity_id: u64,
    /// Mesh index in the source glTF document.
    pub mesh_index: u32,
    /// Primitive index within the source mesh.
    pub primitive_index: u32,
    /// Joint count declared by the selected glTF skin, including unsupported sizes.
    pub joint_count: u32,
    /// Source document asset identity.
    pub document: AssetId,
    /// Derived skeleton asset identity.
    pub skeleton: Option<AssetId>,
    /// Entity transform in world space, stored column-major.
    pub world_transform: [[f32; 4]; 4],
}

/// Stable immutable inputs to one animation extraction frame.
#[derive(Clone, Debug)]
pub struct AnimationPrimitiveSnapshot {
    /// Primitive mappings captured from the scene.
    pub primitives: Arc<[AnimationPrimitiveMapping]>,
}

/// Per-frame animation inputs. Renderer consumers receive only the output frame.
pub struct AnimationContext<'a> {
    /// Immutable scene generation snapshot.
    pub snapshot: SceneSnapshot,
    /// Asset resolver pinned for this extraction boundary.
    pub assets: &'a dyn super::AnimationAssets,
    /// Immutable primitive mapping snapshot.
    pub primitives: &'a AnimationPrimitiveSnapshot,
    /// Camera captured for the same frame boundary.
    pub camera: AnimationCameraFrame,
    /// Scene generation expected by this frame.
    pub scene_generation: u64,
    /// Monotonic frame identifier.
    pub frame_id: u64,
}

/// Column-major palette padded to the GPU's 128-joint storage ABI.
#[derive(Clone, Debug, PartialEq)]
pub struct JointPalette {
    /// Active matrices in stable skin joint order.
    pub matrices: Vec<Mat4>,
}

/// WGSL-compatible storage-buffer payload for one primitive palette.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct JointPaletteGpu {
    /// 128 column-major `mat4x4<f32>` entries, padded with identity matrices.
    pub matrices: [[[f32; 4]; 4]; 128],
    /// Number of valid matrices in `matrices`.
    pub joint_count: u32,
    /// Explicit tail padding keeps the storage struct 16-byte aligned.
    pub padding: [u32; 3],
}

impl JointPaletteGpu {
    /// Pack a validated CPU palette into the fixed-size GPU storage ABI.
    pub fn from_palette(palette: &JointPalette) -> Result<Self, AnimationError> {
        if palette.matrices.is_empty() || palette.matrices.len() > 128 {
            return Err(AnimationError::InvalidPaletteSize(palette.matrices.len()));
        }
        let mut matrices = [Mat4::IDENTITY.to_cols_array_2d(); 128];
        for (target, matrix) in matrices.iter_mut().zip(&palette.matrices) {
            if !matrix_is_finite(*matrix) {
                return Err(AnimationError::NonFiniteMatrix);
            }
            *target = matrix.to_cols_array_2d();
        }
        Ok(Self {
            matrices,
            joint_count: palette.matrices.len() as u32,
            padding: [0; 3],
        })
    }
}

/// One render item after pose, palette, and animated bounds extraction.
#[derive(Clone, Debug)]
pub struct AnimationRenderItem {
    /// Source entity identity.
    pub entity_id: u64,
    /// Source primitive index.
    pub primitive_index: u32,
    /// Per-item skinning result.
    pub result: AnimationItemResult,
}

/// Successful skinned data or a local static fallback.
#[derive(Clone, Debug)]
pub enum AnimationItemResult {
    /// Valid palette and current-pose world bounds.
    Skinned {
        /// Fixed GPU ABI data for this primitive.
        palette: Arc<JointPaletteGpu>,
        /// Current-pose world bounds shared by all geometry consumers.
        world_bounds: Aabb,
    },
    /// Keep this primitive on the existing static-mesh path.
    StaticFallback(StaticFallbackReason),
}

/// Reason one primitive fell back without invalidating other frame items.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StaticFallbackReason {
    /// This primitive's skin is larger than the supported palette.
    JointLimit,
    /// Runtime pose data does not exactly match its skeleton.
    MissingPose,
    /// The skin data references a joint outside its palette.
    JointOutOfRange,
    /// A transform or influence stream contains invalid numbers.
    InvalidNumericData,
    /// The primitive does not have the required skin streams or vertices.
    MissingSkinStreams,
}

/// Structured diagnostic for one item fallback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnimationDiagnostic {
    /// Affected entity.
    pub entity_id: u64,
    /// Affected primitive.
    pub primitive_index: u32,
    /// Stable fallback reason.
    pub reason: StaticFallbackReason,
}

/// Published immutable animation outputs for a single scene generation/frame.
#[derive(Clone, Debug)]
pub struct AnimationFrame {
    /// Scene generation that owns all items in this frame.
    pub scene_generation: u64,
    /// Frame id for deterministic boundary ordering.
    pub frame_id: u64,
    /// Extracted per-primitive render data.
    pub items: Arc<[AnimationRenderItem]>,
    /// Item-local fallback diagnostics.
    pub diagnostics: Arc<[AnimationDiagnostic]>,
}

/// Mutable pose state owned by the animation runtime, not by renderer passes.
#[derive(Debug)]
pub struct AnimatorRuntime {
    pub(crate) generation: u64,
    pub(crate) poses: BTreeMap<u64, Arc<[LocalPose]>>,
    pub(crate) last_published_frame: Option<u64>,
}

impl AnimatorRuntime {
    /// Create a runtime pinned to one scene generation.
    pub fn new(generation: u64) -> Self {
        Self {
            generation,
            poses: BTreeMap::new(),
            last_published_frame: None,
        }
    }

    /// Replace the sampled joint-local poses for one entity.
    pub fn set_pose(&mut self, entity_id: u64, pose: Vec<LocalPose>) {
        self.poses.insert(entity_id, Arc::from(pose));
    }

    /// Advance to a new scene generation and clear all old poses/publication state.
    pub fn reset_generation(&mut self, generation: u64) {
        self.generation = generation;
        self.poses.clear();
        self.last_published_frame = None;
    }
}

/// Context-level errors reject the entire frame and publish no items.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum AnimationError {
    /// Runtime, immutable scene snapshot, or requested frame generation differs.
    #[error("animation scene generation is stale: runtime={runtime}, snapshot={snapshot}, requested={requested}")]
    StaleGeneration {
        /// Runtime generation.
        runtime: u64,
        /// Snapshot generation.
        snapshot: u64,
        /// Requested generation.
        requested: u64,
    },
    /// Requested output would move frame publication backwards or repeat it.
    #[error("animation frame {requested} is not newer than published frame {last}")]
    StaleFrame {
        /// Last published frame id.
        last: u64,
        /// Requested frame id.
        requested: u64,
    },
    /// A document or skeleton required by a mapping is unavailable.
    #[error("animation asset is missing: {kind} {id}")]
    MissingAsset {
        /// Missing asset category.
        kind: &'static str,
        /// Missing asset identity.
        id: String,
    },
    /// A mapping references a missing mesh or primitive in an owned document.
    #[error("animation primitive mapping is invalid: entity={entity_id}, mesh={mesh_index}, primitive={primitive_index}")]
    InvalidPrimitiveMapping {
        /// Affected entity.
        entity_id: u64,
        /// Source mesh index.
        mesh_index: u32,
        /// Source primitive index.
        primitive_index: u32,
    },
    /// Palette storage only supports 1..=128 matrices.
    #[error("joint palette size {0} is outside 1..=128")]
    InvalidPaletteSize(usize),
    /// Pose joint count does not match the selected skeleton.
    #[error("animation pose contains {actual} joints; skeleton requires {expected}")]
    PoseJointCountMismatch {
        /// Required skeleton joint count.
        expected: usize,
        /// Provided pose joint count.
        actual: usize,
    },
    /// Palette or world-space transform contains NaN or infinity.
    #[error("animation matrix contains a non-finite value")]
    NonFiniteMatrix,
    /// Node hierarchy is invalid despite the document validation boundary.
    #[error("glTF node graph cannot be evaluated")]
    InvalidNodeGraph,
    /// A clip or playback value is internally inconsistent.
    #[error("invalid animation clip: {0}")]
    InvalidClip(String),
}

pub(crate) fn matrix_is_finite(matrix: Mat4) -> bool {
    matrix.to_cols_array().into_iter().all(f32::is_finite)
}
