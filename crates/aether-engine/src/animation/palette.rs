use super::types::{matrix_is_finite, AnimationError, JointPalette, LocalPose};
use crate::asset::loaders::gltf::{GltfDocumentAsset, GltfPrimitive, SkeletonAsset};
use crate::math::{Aabb, Mat4, Vec3};

/// Build a stable joint palette from sampled local poses and glTF node ancestry.
///
/// Matrix arrays remain column-major. Each entry follows glTF's column-vector
/// convention: `global_joint * inverse_bind`.
pub fn build_joint_palette(
    document: &GltfDocumentAsset,
    skeleton: &SkeletonAsset,
    pose: &[LocalPose],
) -> Result<JointPalette, AnimationError> {
    if skeleton.joints.is_empty() || skeleton.joints.len() > 128 {
        return Err(AnimationError::InvalidPaletteSize(skeleton.joints.len()));
    }
    if pose.len() != skeleton.joints.len() {
        return Err(AnimationError::PoseJointCountMismatch {
            expected: skeleton.joints.len(),
            actual: pose.len(),
        });
    }
    if skeleton.inverse_bind.len() != skeleton.joints.len() {
        return Err(AnimationError::InvalidNodeGraph);
    }

    let mut node_to_joint = vec![None; document.nodes.len()];
    for (joint_index, joint) in skeleton.joints.iter().enumerate() {
        let slot = node_to_joint
            .get_mut(joint.node_index as usize)
            .ok_or(AnimationError::InvalidNodeGraph)?;
        *slot = Some(joint_index);
    }

    let mut globals = vec![None; document.nodes.len()];
    let mut visiting = vec![false; document.nodes.len()];
    let mut matrices = Vec::with_capacity(skeleton.joints.len());
    for (joint_index, joint) in skeleton.joints.iter().enumerate() {
        let node_index = joint.node_index as usize;
        let global = global_node_matrix(
            document,
            node_index,
            &node_to_joint,
            pose,
            &mut globals,
            &mut visiting,
        )?;
        let inverse_bind = skeleton.inverse_bind[joint_index];
        if !matrix_is_finite(inverse_bind) {
            return Err(AnimationError::NonFiniteMatrix);
        }
        let palette_matrix = global * inverse_bind;
        if !matrix_is_finite(palette_matrix) {
            return Err(AnimationError::NonFiniteMatrix);
        }
        matrices.push(palette_matrix);
    }
    Ok(JointPalette { matrices })
}

fn global_node_matrix(
    document: &GltfDocumentAsset,
    index: usize,
    node_to_joint: &[Option<usize>],
    pose: &[LocalPose],
    cache: &mut [Option<Mat4>],
    visiting: &mut [bool],
) -> Result<Mat4, AnimationError> {
    if let Some(matrix) = cache.get(index).copied().flatten() {
        return Ok(matrix);
    }
    let node = document
        .nodes
        .get(index)
        .ok_or(AnimationError::InvalidNodeGraph)?;
    let visited = visiting
        .get_mut(index)
        .ok_or(AnimationError::InvalidNodeGraph)?;
    if *visited {
        return Err(AnimationError::InvalidNodeGraph);
    }
    *visited = true;

    let local = match node_to_joint.get(index).copied().flatten() {
        Some(joint_index) => pose
            .get(joint_index)
            .copied()
            .and_then(LocalPose::matrix)
            .ok_or(AnimationError::NonFiniteMatrix)?,
        None => Mat4::from_cols_array_2d(&node.local_matrix),
    };
    if !matrix_is_finite(local) {
        return Err(AnimationError::NonFiniteMatrix);
    }
    let global = if let Some(parent) = node.parent {
        global_node_matrix(
            document,
            parent as usize,
            node_to_joint,
            pose,
            cache,
            visiting,
        )? * local
    } else {
        local
    };
    if !matrix_is_finite(global) {
        return Err(AnimationError::NonFiniteMatrix);
    }
    visiting[index] = false;
    cache[index] = Some(global);
    Ok(global)
}

/// Compute current-pose world bounds using the same weighted palette as the shader.
pub fn skinned_world_bounds(
    primitive: &GltfPrimitive,
    palette: &JointPalette,
    world_transform: Mat4,
) -> Result<Aabb, super::types::StaticFallbackReason> {
    let joints = primitive
        .joints
        .as_ref()
        .ok_or(super::types::StaticFallbackReason::MissingSkinStreams)?;
    let weights = primitive
        .weights
        .as_ref()
        .ok_or(super::types::StaticFallbackReason::MissingSkinStreams)?;
    if primitive.positions.is_empty()
        || joints.len() != primitive.positions.len()
        || weights.len() != primitive.positions.len()
    {
        return Err(super::types::StaticFallbackReason::MissingSkinStreams);
    }
    if !matrix_is_finite(world_transform) {
        return Err(super::types::StaticFallbackReason::InvalidNumericData);
    }

    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for (index, position) in primitive.positions.iter().enumerate() {
        let position = Vec3::from_array(*position);
        let vertex_weights = weights[index];
        if !position.is_finite() || vertex_weights.iter().any(|weight| !weight.is_finite()) {
            return Err(super::types::StaticFallbackReason::InvalidNumericData);
        }
        let mut skinned_position = Vec3::ZERO;
        let mut total_weight = 0.0;
        for influence in 0..4 {
            let weight = vertex_weights[influence];
            if weight < 0.0 {
                return Err(super::types::StaticFallbackReason::InvalidNumericData);
            }
            if weight <= 1.0e-8 {
                continue;
            }
            let joint = joints[index][influence] as usize;
            let matrix = palette
                .matrices
                .get(joint)
                .ok_or(super::types::StaticFallbackReason::JointOutOfRange)?;
            let transformed = matrix.transform_point3(position);
            if !transformed.is_finite() {
                return Err(super::types::StaticFallbackReason::InvalidNumericData);
            }
            skinned_position += transformed * weight;
            total_weight += weight;
        }
        if !total_weight.is_finite() || total_weight < 1.0e-6 {
            return Err(super::types::StaticFallbackReason::InvalidNumericData);
        }
        let world_position = world_transform.transform_point3(skinned_position);
        if !world_position.is_finite() {
            return Err(super::types::StaticFallbackReason::InvalidNumericData);
        }
        min = min.min(world_position);
        max = max.max(world_position);
    }
    Ok(Aabb::new(min, max))
}
