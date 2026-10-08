//! Joint-palette construction and conservative animated bounds.

use super::player::{AnimationError, LocalPose, PoseSample};
use crate::asset::gltf_document::{GltfDocument, SkeletonAsset, MAX_SKELETON_JOINTS};
use crate::asset::store::AssetId;
use crate::math::Aabb;
use glam::Mat4;
use std::sync::Arc;

/// Immutable joint matrices and shared animated bounds for one render item.
#[derive(Debug, Clone)]
pub struct JointPalette {
    /// Persistent skeleton identity.
    pub skeleton: AssetId,
    /// Column-major matrices indexed by skeleton joint order.
    pub matrices: Arc<[Mat4]>,
    /// Scene generation that produced this palette.
    pub generation: u64,
    /// Conservative world-space bounds consumed by all render passes.
    pub bounds: Aabb,
}

/// Build `inverse_bind × global_joint` matrices and conservative world bounds.
pub fn build_joint_palette(
    document: &GltfDocument,
    skeleton: &SkeletonAsset,
    pose: &PoseSample,
    bind_bounds: Aabb,
    world: Mat4,
    generation: u64,
) -> Result<Arc<JointPalette>, AnimationError> {
    let joint_count = skeleton.joints.len();
    if joint_count == 0
        || joint_count > MAX_SKELETON_JOINTS
        || skeleton.inverse_bind_matrices.len() != joint_count
        || pose.locals.len() != document.nodes.len()
    {
        return Err(AnimationError::InvalidPalette);
    }
    let mut globals = vec![None; document.nodes.len()];
    let mut visiting = vec![false; document.nodes.len()];
    let mut matrices = Vec::with_capacity(joint_count);
    let mut bounds_min = glam::Vec3::splat(f32::INFINITY);
    let mut bounds_max = glam::Vec3::splat(f32::NEG_INFINITY);
    for (joint_index, node_index) in skeleton.joints.iter().copied().enumerate() {
        let global = node_global(
            node_index,
            document,
            &pose.locals,
            &mut globals,
            &mut visiting,
        )?;
        let inverse_bind = Mat4::from_cols_array_2d(&skeleton.inverse_bind_matrices[joint_index]);
        let palette = inverse_bind * global;
        if !palette.is_finite() {
            return Err(AnimationError::InvalidPalette);
        }
        let joint_bounds = bind_bounds.transform(palette);
        bounds_min = bounds_min.min(joint_bounds.min);
        bounds_max = bounds_max.max(joint_bounds.max);
        matrices.push(palette);
    }
    let bounds = Aabb::new(bounds_min, bounds_max).transform(world);
    if !bounds.min.is_finite() || !bounds.max.is_finite() {
        return Err(AnimationError::InvalidPalette);
    }
    Ok(Arc::new(JointPalette {
        skeleton: skeleton
            .source
            .clone()
            .ok_or(AnimationError::MissingAsset)?,
        matrices: Arc::from(matrices),
        generation,
        bounds,
    }))
}

fn node_global(
    index: usize,
    document: &GltfDocument,
    locals: &[LocalPose],
    globals: &mut [Option<Mat4>],
    visiting: &mut [bool],
) -> Result<Mat4, AnimationError> {
    if let Some(global) = globals.get(index).and_then(|value| *value) {
        return Ok(global);
    }
    let node = document
        .nodes
        .get(index)
        .ok_or(AnimationError::MissingJoint(index))?;
    if visiting[index] {
        return Err(AnimationError::Document(
            crate::asset::gltf_document::ValidationError::Cycle,
        ));
    }
    visiting[index] = true;
    let local = locals
        .get(index)
        .ok_or(AnimationError::MissingJoint(index))?
        .matrix();
    let global = if let Some(parent) = node.parent {
        node_global(parent, document, locals, globals, visiting)? * local
    } else {
        local
    };
    visiting[index] = false;
    globals[index] = Some(global);
    Ok(global)
}

#[cfg(test)]
#[path = "palette_tests.rs"]
mod tests;
