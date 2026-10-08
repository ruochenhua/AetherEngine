//! Owned glTF data needed by skinning and animation runtimes.

use super::mesh::CpuMesh;
use super::store::{AssetId, AssetKind, TypedAsset};
use super::Asset;
use std::path::Path;
use std::sync::Arc;

mod loader;
mod validation;
use validation::{validate_animation, validate_skin};

/// Maximum supported joints in a renderer palette.
pub const MAX_SKELETON_JOINTS: usize = 128;

/// Owned glTF document data. Large decoded and structured collections are shared by `Arc`.
#[derive(Debug, Clone, Default)]
pub struct GltfDocument {
    /// Imported binary buffers, including geometry and animation accessor data.
    pub buffers: Arc<Vec<Arc<[u8]>>>,
    /// Imported image bytes, retained without coupling consumers to the decoder.
    pub images: Arc<Vec<Arc<[u8]>>>,
    /// Node hierarchy and links into meshes and skins.
    pub nodes: Arc<Vec<DocumentNode>>,
    /// Meshes and their per-primitive skin weights.
    pub meshes: Arc<Vec<DocumentMesh>>,
    /// Skin joints and inverse bind matrices.
    pub skins: Arc<Vec<DocumentSkin>>,
    /// Animation channels with owned keyframes.
    pub animations: Arc<Vec<DocumentAnimation>>,
}

/// Owned node metadata used to validate and evaluate a glTF scene graph.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocumentNode {
    /// Child node indexes.
    pub children: Vec<usize>,
    /// Parent node index, when this node is not a root.
    pub parent: Option<usize>,
    /// Optional mesh index.
    pub mesh: Option<usize>,
    /// Optional skin index attached to this node.
    pub skin: Option<usize>,
    /// Bind-pose translation.
    pub translation: [f32; 3],
    /// Bind-pose quaternion in XYZW order.
    pub rotation: [f32; 4],
    /// Bind-pose scale.
    pub scale: [f32; 3],
}

/// Owned mesh metadata.
#[derive(Debug, Clone, Default)]
pub struct DocumentMesh {
    /// Mesh primitive data retained in source order.
    pub primitives: Vec<DocumentPrimitive>,
}

/// Per-primitive skin data. The mesh accessor bytes remain available in `GltfDocument::buffers`.
#[derive(Debug, Clone, Default)]
pub struct DocumentPrimitive {
    /// Decoded static geometry used by the renderer and static fallback path.
    pub mesh: Option<CpuMesh>,
    /// Four-component JOINTS/WEIGHTS pairs for each vertex, when present.
    pub weights: Vec<JointWeights>,
}

/// Four influences attached to one vertex.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct JointWeights {
    /// Joint indexes local to the primitive's skin.
    pub joints: [u16; 4],
    /// Influence weights, normalized when their sum differs materially from one.
    pub weights: [f32; 4],
}

impl JointWeights {
    /// Validate and normalize one vertex's four skin influences.
    pub fn validated(joints: [u16; 4], mut weights: [f32; 4]) -> Result<Self, ValidationError> {
        if weights
            .iter()
            .any(|weight| !weight.is_finite() || *weight < 0.0)
        {
            return Err(ValidationError::InvalidWeight);
        }
        let sum: f32 = weights.iter().sum();
        if !sum.is_finite() || sum < 1.0e-6 {
            return Err(ValidationError::WeightSumTooSmall);
        }
        if (sum - 1.0).abs() > 1.0e-4 {
            for weight in &mut weights {
                *weight /= sum;
            }
        }
        Ok(Self { joints, weights })
    }
}

/// Owned skin data.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocumentSkin {
    /// Node indexes that form the skin palette.
    pub joints: Vec<usize>,
    /// Column-major inverse bind matrices, one per joint.
    pub inverse_bind_matrices: Vec<[[f32; 4]; 4]>,
}

/// Typed runtime-ready skeleton asset.
#[derive(Debug, Clone, PartialEq)]
pub struct SkeletonAsset {
    /// Persistent asset identity for palette caching, when loaded from a store.
    pub source: Option<AssetId>,
    /// Node indexes that form the ordered palette.
    pub joints: Vec<usize>,
    /// Column-major inverse bind matrices, one per joint.
    pub inverse_bind_matrices: Vec<[[f32; 4]; 4]>,
}

impl SkeletonAsset {
    /// Build a skeleton after validating its joint count, node references, and bind matrices.
    pub fn from_skin(document: &GltfDocument, skin_index: usize) -> Result<Self, ValidationError> {
        let skin = document
            .skins
            .get(skin_index)
            .ok_or(ValidationError::InvalidReference)?;
        validate_skin(document, skin_index, skin, true)?;
        Ok(Self {
            source: None,
            joints: skin.joints.clone(),
            inverse_bind_matrices: skin.inverse_bind_matrices.clone(),
        })
    }

    /// Build a skeleton and retain its persistent asset identity.
    pub fn from_skin_with_source(
        document: &GltfDocument,
        skin_index: usize,
        source: AssetId,
    ) -> Result<Self, ValidationError> {
        let mut skeleton = Self::from_skin(document, skin_index)?;
        skeleton.source = Some(source);
        Ok(skeleton)
    }
}

/// Animation target property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationPath {
    /// Translation vector, three components.
    Translation,
    /// Rotation quaternion, four components.
    Rotation,
    /// Scale vector, three components.
    Scale,
}

/// Target property alias used by the animation runtime.
pub type ChannelTarget = AnimationPath;

/// Keyframe interpolation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    /// Hold the previous key until the next key.
    Step,
    /// Linear interpolation; rotations should use quaternion slerp.
    Linear,
    /// Cubic spline values are stored as in/value/out triplets per time.
    CubicSpline,
}

/// Typed keyframe values retained in shared immutable slices.
#[derive(Debug, Clone, PartialEq)]
pub enum AnimationValues {
    /// Translation or scale key values.
    Vec3(Arc<[[f32; 3]]>),
    /// Quaternion key values in XYZW order.
    Vec4(Arc<[[f32; 4]]>),
}

/// Owned animation channel with unpacked keyframe values.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationChannel {
    /// Target node index (the joint node when it belongs to a skin).
    pub target_joint: usize,
    /// Animated transform property.
    pub target: ChannelTarget,
    /// Interpolation used between adjacent keys.
    pub interpolation: Interpolation,
    /// Strictly increasing keyframe times in seconds.
    pub times: Arc<[f32]>,
    /// Typed keyframe values; cubic spline stores in/value/out triplets per time.
    pub values: AnimationValues,
}

/// Owned animation clip data.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocumentAnimation {
    /// Channels retained in source order.
    pub channels: Vec<AnimationChannel>,
}

/// Typed runtime-ready animation clip asset.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnimationClipAsset {
    /// Validated channels retained in source order.
    pub channels: Vec<AnimationChannel>,
    /// Maximum channel key time, or zero for an empty clip.
    pub duration: f32,
}

impl AnimationClipAsset {
    /// Validate and clone an owned clip from a document.
    pub fn from_animation(
        document: &GltfDocument,
        animation_index: usize,
    ) -> Result<Self, ValidationError> {
        let animation = document
            .animations
            .get(animation_index)
            .ok_or(ValidationError::InvalidReference)?;
        validate_animation(document, animation)?;
        Ok(Self {
            channels: animation.channels.clone(),
            duration: animation.duration(),
        })
    }
}

/// Validation errors that prevent publication of typed skeleton/animation assets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    /// Node graph contains a cycle.
    #[error("node graph contains a cycle")]
    Cycle,
    /// A node, mesh, skin, or animation reference is invalid.
    #[error("document contains an invalid index reference")]
    InvalidReference,
    /// Inverse-bind matrix count does not equal the skin joint count.
    #[error("inverse-bind matrix count does not match joint count")]
    InverseBindCount,
    /// An inverse-bind matrix contains non-finite components.
    #[error("inverse-bind matrix contains non-finite values")]
    InvalidInverseBind,
    /// Skeleton exceeds the supported palette size.
    #[error("unsupported joint count: {count}")]
    UnsupportedJointCount {
        /// Number of source joints.
        count: usize,
    },
    /// A vertex influence contains a negative or non-finite weight.
    #[error("skin weight must be finite and non-negative")]
    InvalidWeight,
    /// A vertex influence has effectively zero total weight.
    #[error("skin weight sum is too small")]
    WeightSumTooSmall,
    /// Animation key count and value count differ.
    #[error("animation input and output key counts differ")]
    ChannelCount,
    /// Animation value arity does not match the target property.
    #[error("animation output arity does not match target property")]
    ChannelArity,
    /// Animation key times or values contain invalid numbers or ordering.
    #[error("animation key data must be finite and times strictly increasing")]
    InvalidChannelData,
}

/// Non-fatal findings that select a static fallback for the affected primitive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationDiagnostic {
    /// Primitive references a skin larger than the supported palette limit.
    UnsupportedJointCount {
        /// Skin index.
        skin: usize,
        /// Mesh and primitive indexes.
        primitive: (usize, usize),
        /// Full source joint count (never truncated).
        joint_count: usize,
    },
}

/// Results from document-level validation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationReport {
    /// Per-primitive diagnostics requiring static rendering fallback.
    pub diagnostics: Vec<ValidationDiagnostic>,
}
