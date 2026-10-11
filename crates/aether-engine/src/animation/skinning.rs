//! Validation for vertex skin inputs uploaded to the GPU renderer.

use super::{AnimationError, JointPalette};
use crate::asset::gltf_document::JointWeights;

/// Reject malformed influences before they reach shader palette indexing.
pub(crate) fn validate_skinning_inputs(
    vertex_count: usize,
    weights: &[JointWeights],
    palette: &JointPalette,
) -> Result<(), AnimationError> {
    if vertex_count != weights.len() || palette.matrices.is_empty() {
        return Err(AnimationError::InvalidPalette);
    }
    for influence in weights {
        let mut total = 0.0;
        for slot in 0..4 {
            let weight = influence.weights[slot];
            if !weight.is_finite() || weight < 0.0 {
                return Err(AnimationError::Document(
                    crate::asset::gltf_document::ValidationError::InvalidWeight,
                ));
            }
            if weight > 0.0 {
                let matrix = palette
                    .matrices
                    .get(influence.joints[slot] as usize)
                    .ok_or(AnimationError::InvalidPalette)?;
                if !matrix.is_finite() {
                    return Err(AnimationError::InvalidPalette);
                }
            }
            total += weight;
        }
        if !total.is_finite() || total <= f32::EPSILON {
            return Err(AnimationError::Document(
                crate::asset::gltf_document::ValidationError::WeightSumTooSmall,
            ));
        }
    }
    Ok(())
}
