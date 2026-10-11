use super::validate_skinning_inputs;
use crate::animation::JointPalette;
use crate::asset::gltf_document::JointWeights;
use crate::asset::store::{canonicalize, AssetId, AssetKind};
use crate::math::{Aabb, Mat4, Vec3};
use std::path::Path;
use std::sync::Arc;

fn palette(matrices: Vec<Mat4>) -> JointPalette {
    JointPalette {
        skeleton: AssetId {
            kind: AssetKind::Skeleton,
            path: canonicalize(Path::new("."), Path::new("rig.gltf")).unwrap(),
        },
        matrices: Arc::from(matrices),
        generation: 1,
        bounds: Aabb::new(Vec3::ZERO, Vec3::ONE),
    }
}

#[test]
fn skinning_inputs_accept_valid_four_joint_influences() {
    let weights = [JointWeights::validated([0, 1, 0, 0], [0.25, 0.75, 0.0, 0.0]).unwrap()];
    assert!(validate_skinning_inputs(1, &weights, &palette(vec![Mat4::IDENTITY; 2])).is_ok());
}

#[test]
fn skinning_inputs_reject_bad_vertex_count_and_palette_indices() {
    let weights = [JointWeights::validated([0; 4], [1.0, 0.0, 0.0, 0.0]).unwrap()];
    assert!(validate_skinning_inputs(2, &weights, &palette(vec![Mat4::IDENTITY])).is_err());

    let invalid = [JointWeights {
        joints: [1, 0, 0, 0],
        weights: [1.0, 0.0, 0.0, 0.0],
    }];
    assert!(validate_skinning_inputs(1, &invalid, &palette(vec![Mat4::IDENTITY])).is_err());
}
