use crate::asset::mesh::{CpuMesh, SkinnedVertex};
use glam::Mat4;
use std::sync::Arc;
use thiserror::Error;

/// Fully owned glTF data. No parser objects or borrowed buffer views escape import.
#[derive(Clone, Debug)]
pub struct GltfDocumentAsset {
    /// Source buffer bytes retained for accessor-backed downstream consumers.
    pub buffers: Vec<Arc<[u8]>>,
    /// Decoded image payloads, including their source format and dimensions.
    pub images: Vec<Arc<GltfImage>>,
    /// Nodes in stable document order, with validated parent/child links.
    pub nodes: Vec<Arc<GltfNode>>,
    /// Meshes and their decoded primitive vertex/index streams.
    pub meshes: Vec<Arc<GltfMesh>>,
    /// Skin joint order and inverse bind matrices.
    pub skins: Vec<Arc<GltfSkin>>,
    /// Validated Step/Linear animation channels.
    pub animations: Vec<Arc<GltfAnimation>>,
}

/// Decoded glTF image data in its original channel/depth representation.
#[derive(Clone, Debug)]
pub struct GltfImage {
    /// Optional authored name.
    pub name: Option<String>,
    /// Pixel payload returned by the glTF image importer.
    pub pixels: Arc<[u8]>,
    /// Pixel format.
    pub format: GltfImageFormat,
    /// Image width.
    pub width: u32,
    /// Image height.
    pub height: u32,
}

/// Pixel formats supported by the glTF importer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GltfImageFormat {
    /// 8-bit channel layouts.
    R8,
    /// 8-bit channel layouts.
    Rg8,
    /// 8-bit channel layouts.
    Rgb8,
    /// 8-bit channel layouts.
    Rgba8,
    /// 16-bit channel layouts.
    R16,
    /// 16-bit channel layouts.
    Rg16,
    /// 16-bit channel layouts.
    Rgb16,
    /// 16-bit channel layouts.
    Rgba16,
    /// 32-bit floating-point RGB.
    Rgb32Float,
    /// 32-bit floating-point RGBA.
    Rgba32Float,
}

/// A glTF node with stable indices and both matrix and decomposed bind transform.
#[derive(Clone, Debug)]
pub struct GltfNode {
    /// Optional authored name.
    pub name: Option<String>,
    /// Parent node index, when present.
    pub parent: Option<u32>,
    /// Child node indices in document order.
    pub children: Arc<[u32]>,
    /// Optional mesh index.
    pub mesh: Option<u32>,
    /// Optional skin index.
    pub skin: Option<u32>,
    /// Column-major local transform matrix.
    pub local_matrix: [[f32; 4]; 4],
    /// Bind translation.
    pub translation: [f32; 3],
    /// Bind rotation in glTF `[x, y, z, w]` order.
    pub rotation: [f32; 4],
    /// Bind scale.
    pub scale: [f32; 3],
}

/// A glTF mesh with its primitives kept separate.
#[derive(Clone, Debug)]
pub struct GltfMesh {
    /// Optional authored name.
    pub name: Option<String>,
    /// Mesh primitives in document order.
    pub primitives: Arc<[GltfPrimitive]>,
}

/// Decoded vertex streams for one triangle primitive.
#[derive(Clone, Debug)]
pub struct GltfPrimitive {
    /// Positions in mesh-local coordinates.
    pub positions: Arc<[[f32; 3]]>,
    /// Optional authored normals.
    pub normals: Option<Arc<[[f32; 3]]>>,
    /// Optional authored tangents and handedness.
    pub tangents: Option<Arc<[[f32; 4]]>>,
    /// Optional first texture coordinate set.
    pub tex_coords: Option<Arc<[[f32; 2]]>>,
    /// Indices, or the implicit sequential indices when none were authored.
    pub indices: Arc<[u32]>,
    /// Optional JOINTS_0 values converted to unsigned 32-bit indices.
    pub joints: Option<Arc<[[u32; 4]]>>,
    /// Optional normalized WEIGHTS_0 values.
    pub weights: Option<Arc<[[f32; 4]]>>,
}

impl GltfPrimitive {
    /// Interleave this primitive's validated skin streams for GPU upload.
    ///
    /// Static primitives return `None`; missing optional PBR attributes use
    /// the same stable defaults as the existing CPU mesh path.
    pub fn skinned_vertices(&self) -> Option<Vec<SkinnedVertex>> {
        let joints = self.joints.as_ref()?;
        let weights = self.weights.as_ref()?;
        if joints.len() != self.positions.len() || weights.len() != self.positions.len() {
            return None;
        }
        Some(
            self.positions
                .iter()
                .enumerate()
                .map(|(index, position)| SkinnedVertex {
                    position: *position,
                    normal: self
                        .normals
                        .as_ref()
                        .and_then(|values| values.get(index))
                        .copied()
                        .unwrap_or([0.0, 1.0, 0.0]),
                    uv: self
                        .tex_coords
                        .as_ref()
                        .and_then(|values| values.get(index))
                        .copied()
                        .unwrap_or([0.0; 2]),
                    tangent: self
                        .tangents
                        .as_ref()
                        .and_then(|values| values.get(index))
                        .copied()
                        .unwrap_or([1.0, 0.0, 0.0, 1.0]),
                    joints: joints[index],
                    weights: weights[index],
                })
                .collect(),
        )
    }

    /// Preserve this primitive as a static CPU mesh when skinning is unsupported.
    pub fn static_mesh_fallback(&self) -> CpuMesh {
        let mut normals = self
            .normals
            .as_ref()
            .map(|values| values.to_vec())
            .unwrap_or_default();
        if normals.is_empty() {
            crate::asset::loaders::compute_smooth_normals(
                &self.positions,
                &self.indices,
                &mut normals,
            );
        }
        let mut uvs = self
            .tex_coords
            .as_ref()
            .map(|values| values.to_vec())
            .unwrap_or_default();
        crate::asset::loaders::fill_missing_uvs(&mut uvs, self.positions.len());
        let tangents = self
            .tangents
            .as_ref()
            .map(|values| values.to_vec())
            .unwrap_or_else(|| {
                crate::asset::loaders::compute_tangents(
                    &self.positions,
                    &normals,
                    &uvs,
                    &self.indices,
                )
            });
        CpuMesh {
            positions: self.positions.to_vec(),
            normals,
            uvs,
            tangents,
            indices: self.indices.to_vec(),
            submeshes: Vec::new(),
        }
    }
}

/// A skin with joint node references and column-major inverse bind matrices.
#[derive(Clone, Debug)]
pub struct GltfSkin {
    /// Optional authored name.
    pub name: Option<String>,
    /// Skeleton root node, when authored.
    pub skeleton: Option<u32>,
    /// Joint node indices in palette order.
    pub joints: Arc<[u32]>,
    /// Inverse bind matrices in the same order; identity is used when omitted.
    pub inverse_bind: Arc<[Mat4]>,
}

/// A parsed animation and its typed joint-independent channels.
#[derive(Clone, Debug)]
pub struct GltfAnimation {
    /// Optional authored name.
    pub name: Option<String>,
    /// Channels in document order, targeting glTF node indices.
    pub channels: Arc<[GltfAnimationChannel]>,
    /// Largest channel key time, or zero for an empty animation.
    pub duration: f32,
}

/// An animation channel retaining its glTF node target until paired with a skin.
#[derive(Clone, Debug)]
pub struct GltfAnimationChannel {
    /// Target node index in the source document.
    pub target_node: u32,
    /// Target property.
    pub target: ChannelTarget,
    /// Strictly increasing finite key times.
    pub times: Arc<[f32]>,
    /// Values whose variant must match `target`.
    pub values: AnimationValues,
    /// Supported key interpolation rule.
    pub interpolation: Interpolation,
}

/// A typed channel after mapping its target node to a skeleton joint.
#[derive(Clone, Debug)]
pub struct AnimationChannel {
    /// Target joint in skin palette order.
    pub target_joint: u32,
    /// Target property.
    pub target: ChannelTarget,
    /// Strictly increasing finite key times.
    pub times: Arc<[f32]>,
    /// Typed channel values.
    pub values: AnimationValues,
    /// Supported key interpolation rule.
    pub interpolation: Interpolation,
}

/// Animation property targeted by a channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChannelTarget {
    /// Local translation.
    Translation,
    /// Local quaternion rotation.
    Rotation,
    /// Local scale.
    Scale,
}

/// Channel values decoded to engine-native glTF value arrays.
#[derive(Clone, Debug, PartialEq)]
pub enum AnimationValues {
    /// Translation keys.
    Translation(Arc<[[f32; 3]]>),
    /// Rotation keys in `[x, y, z, w]` order.
    Rotation(Arc<[[f32; 4]]>),
    /// Scale keys.
    Scale(Arc<[[f32; 3]]>),
}

/// Interpolation behavior supported by the first animation slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Interpolation {
    /// Hold the preceding key until the next key time.
    Step,
    /// Interpolate linearly (rotation uses quaternion slerp downstream).
    Linear,
}

/// Stable validation errors for glTF skeleton and animation data.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum GltfError {
    /// glTF import or an unsupported document feature failed validation.
    #[error("glTF parse failed: {0}")]
    Parse(String),
    /// The node graph contains a cycle.
    #[error("glTF node hierarchy contains a cycle")]
    Cycle,
    /// A vertex influence contains NaN or infinity.
    #[error("glTF vertex weight is not finite")]
    NonFiniteWeight,
    /// A vertex influence is negative.
    #[error("glTF vertex weight is negative")]
    NegativeWeight,
    /// A vertex has no usable positive influence weight.
    #[error("glTF vertex weight sum is below 1e-6")]
    WeightSumTooSmall,
    /// A skin exceeds the supported 128 joint limit.
    #[error("glTF skin has {0} joints; at most 128 are supported")]
    UnsupportedJointCount(u32),
    /// Inverse bind matrix count or values are invalid.
    #[error("glTF inverse bind matrices are invalid")]
    InvalidInverseBind,
    /// Channel times and typed values have different lengths or types.
    #[error("glTF animation channel arity or value type is invalid")]
    ChannelArityMismatch,
    /// Channel input keys are not finite and strictly increasing.
    #[error("glTF animation input times are not strictly increasing")]
    NonMonotonicTime,
    /// A node, accessor, or skin reference violates the document structure.
    #[error("glTF document reference is invalid: {0}")]
    InvalidReference(String),
    /// An animation feature is outside the supported Step/Linear TRS subset.
    #[error("glTF animation feature is unsupported: {0}")]
    UnsupportedAnimation(String),
    /// A clip channel targets a node outside the selected skin.
    #[error("animation channel target is not part of the selected skeleton")]
    ChannelTargetNotInSkeleton,
    /// The primitive topology is outside the first mesh loader slice.
    #[error("glTF primitive topology is unsupported: {0}")]
    UnsupportedPrimitiveMode(String),
    /// The primitive uses more than the supported four skin influences.
    #[error("glTF skin influence set is unsupported: {0}")]
    UnsupportedInfluenceCount(String),
}

impl GltfError {
    /// Stable report code used by slice acceptance evidence.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Parse(_) => "Parse",
            Self::Cycle => "Cycle",
            Self::NonFiniteWeight => "NonFiniteWeight",
            Self::NegativeWeight => "NegativeWeight",
            Self::WeightSumTooSmall => "WeightSumTooSmall",
            Self::UnsupportedJointCount(_) => "UnsupportedJointCount",
            Self::InvalidInverseBind => "InvalidInverseBind",
            Self::ChannelArityMismatch => "ChannelArityMismatch",
            Self::NonMonotonicTime => "NonMonotonicTime",
            Self::InvalidReference(_) => "InvalidReference",
            Self::UnsupportedAnimation(_) => "UnsupportedAnimation",
            Self::ChannelTargetNotInSkeleton => "ChannelTargetNotInSkeleton",
            Self::UnsupportedPrimitiveMode(_) => "UnsupportedPrimitiveMode",
            Self::UnsupportedInfluenceCount(_) => "UnsupportedInfluenceCount",
        }
    }
}

/// Normalize valid four-joint skin weights according to the T7.1 thresholds.
pub(super) fn normalize_weights(weights: &mut [[f32; 4]]) -> Result<(), GltfError> {
    for vertex in weights {
        if vertex.iter().any(|weight| !weight.is_finite()) {
            return Err(GltfError::NonFiniteWeight);
        }
        if vertex.iter().any(|weight| *weight < 0.0) {
            return Err(GltfError::NegativeWeight);
        }
        let sum = vertex.iter().sum::<f32>();
        if !sum.is_finite() {
            return Err(GltfError::NonFiniteWeight);
        }
        if sum < 1.0e-6 {
            return Err(GltfError::WeightSumTooSmall);
        }
        if (sum - 1.0).abs() > 1.0e-4 {
            for weight in vertex {
                *weight /= sum;
            }
        }
    }
    Ok(())
}
