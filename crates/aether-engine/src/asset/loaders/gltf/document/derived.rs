use super::parse::load_document;
use super::types::{AnimationChannel, AnimationValues, GltfDocumentAsset, GltfError};
use crate::asset::{Asset, AssetId, AssetKind};
use glam::Mat4;
use std::path::Path;
use std::sync::Arc;

/// One palette-ordered joint with its validated hierarchy and bind-local TRS.
#[derive(Clone, Debug)]
pub struct Joint {
    /// Source document node index.
    pub node_index: u32,
    /// Optional authored node name.
    pub name: Option<String>,
    /// Parent joint index when the parent is part of this skin.
    pub parent: Option<u32>,
    /// Bind translation.
    pub translation: [f32; 3],
    /// Bind rotation in `[x, y, z, w]` order.
    pub rotation: [f32; 4],
    /// Bind scale.
    pub scale: [f32; 3],
}

/// Immutable skeleton data derived from one glTF skin.
#[derive(Clone, Debug)]
pub struct SkeletonAsset {
    /// Persistent identity of the glTF document that owns this skeleton.
    pub source: AssetId,
    /// Joints in the skin's stable palette order.
    pub joints: Arc<[Joint]>,
    /// Column-major inverse bind matrices in palette order.
    pub inverse_bind: Arc<[Mat4]>,
    /// Palette-order root joint, if the skin contains a root.
    pub root: Option<u32>,
    node_to_joint: Arc<[Option<u32>]>,
}

impl SkeletonAsset {
    /// Build one skeleton from a document skin, rejecting palettes above 128 joints.
    pub fn from_document(
        source: AssetId,
        document: &GltfDocumentAsset,
        skin_index: usize,
    ) -> Result<Self, GltfError> {
        let skin = document.skins.get(skin_index).ok_or_else(|| {
            GltfError::InvalidReference(format!("skin index {skin_index} is out of range"))
        })?;
        let joint_count = u32::try_from(skin.joints.len())
            .map_err(|_| GltfError::UnsupportedJointCount(u32::MAX))?;
        if joint_count > 128 {
            return Err(GltfError::UnsupportedJointCount(joint_count));
        }
        if skin.inverse_bind.len() != skin.joints.len() {
            return Err(GltfError::InvalidInverseBind);
        }

        let mut node_to_joint = vec![None; document.nodes.len()];
        for (joint_index, node_index) in skin.joints.iter().copied().enumerate() {
            let slot = node_to_joint.get_mut(node_index as usize).ok_or_else(|| {
                GltfError::InvalidReference(format!("skin references node {node_index}"))
            })?;
            *slot = Some(joint_index as u32);
        }
        let joints = skin
            .joints
            .iter()
            .copied()
            .map(|node_index| {
                let node = document.nodes.get(node_index as usize).ok_or_else(|| {
                    GltfError::InvalidReference(format!("skin references node {node_index}"))
                })?;
                let parent = node
                    .parent
                    .and_then(|parent_node| node_to_joint.get(parent_node as usize).copied())
                    .flatten();
                Ok(Joint {
                    node_index,
                    name: node.name.clone(),
                    parent,
                    translation: node.translation,
                    rotation: node.rotation,
                    scale: node.scale,
                })
            })
            .collect::<Result<Vec<_>, GltfError>>()?;
        let root = skin
            .skeleton
            .and_then(|node| node_to_joint.get(node as usize).copied())
            .flatten()
            .or_else(|| {
                joints
                    .iter()
                    .position(|joint| joint.parent.is_none())
                    .map(|index| index as u32)
            });

        Ok(Self {
            source,
            joints: Arc::from(joints),
            inverse_bind: Arc::clone(&skin.inverse_bind),
            root,
            node_to_joint: Arc::from(node_to_joint),
        })
    }
}

impl Asset for SkeletonAsset {
    const KIND: AssetKind = AssetKind::Skeleton;

    fn load(path: &Path) -> anyhow::Result<Self> {
        let document = load_document(path).map_err(anyhow::Error::new)?;
        let source = document_id(path)?;
        Self::from_document(source, &document, 0).map_err(anyhow::Error::new)
    }
}

/// A typed glTF animation clip whose channels target palette-ordered joints.
#[derive(Clone, Debug)]
pub struct AnimationClipAsset {
    /// Persistent identity of the source glTF document.
    pub source: AssetId,
    /// Channels mapped from source node indices to skeleton joint indices.
    pub channels: Arc<[AnimationChannel]>,
    /// Largest key time across all channels.
    pub duration: f32,
}

impl AnimationClipAsset {
    /// Build a clip and validate that every animated node belongs to the skeleton.
    pub fn from_document(
        source: AssetId,
        document: &GltfDocumentAsset,
        animation_index: usize,
        skeleton: &SkeletonAsset,
    ) -> Result<Self, GltfError> {
        let animation = document.animations.get(animation_index).ok_or_else(|| {
            GltfError::InvalidReference(format!(
                "animation index {animation_index} is out of range"
            ))
        })?;
        let channels = animation
            .channels
            .iter()
            .map(|channel| {
                let target_joint = skeleton
                    .node_to_joint
                    .get(channel.target_node as usize)
                    .copied()
                    .flatten()
                    .ok_or(GltfError::ChannelTargetNotInSkeleton)?;
                Ok(AnimationChannel {
                    target_joint,
                    target: channel.target,
                    times: Arc::clone(&channel.times),
                    values: clone_values(&channel.values),
                    interpolation: channel.interpolation,
                })
            })
            .collect::<Result<Vec<_>, GltfError>>()?;
        Ok(Self {
            source,
            channels: Arc::from(channels),
            duration: animation.duration,
        })
    }
}

impl Asset for AnimationClipAsset {
    const KIND: AssetKind = AssetKind::AnimationClip;

    fn load(path: &Path) -> anyhow::Result<Self> {
        let document = load_document(path).map_err(anyhow::Error::new)?;
        let source = document_id(path)?;
        let skeleton = SkeletonAsset::from_document(source.clone(), &document, 0)
            .map_err(anyhow::Error::new)?;
        Self::from_document(source, &document, 0, &skeleton).map_err(anyhow::Error::new)
    }
}

impl Asset for GltfDocumentAsset {
    const KIND: AssetKind = AssetKind::GltfDocument;

    fn load(path: &Path) -> anyhow::Result<Self> {
        load_document(path).map_err(anyhow::Error::new)
    }
}

fn document_id(path: &Path) -> anyhow::Result<AssetId> {
    AssetId::from_legacy_external_path(AssetKind::GltfDocument, path).map_err(anyhow::Error::new)
}

fn clone_values(values: &AnimationValues) -> AnimationValues {
    match values {
        AnimationValues::Translation(values) => AnimationValues::Translation(Arc::clone(values)),
        AnimationValues::Rotation(values) => AnimationValues::Rotation(Arc::clone(values)),
        AnimationValues::Scale(values) => AnimationValues::Scale(Arc::clone(values)),
    }
}
