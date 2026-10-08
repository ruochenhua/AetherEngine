use super::gltf_document::{
    AnimationChannel, AnimationPath, AnimationValues, DocumentAnimation, DocumentMesh,
    DocumentNode, DocumentPrimitive, DocumentSkin, GltfDocument, Interpolation, JointWeights,
    SkeletonAsset, ValidationDiagnostic, ValidationError,
};
use std::sync::Arc;

fn fixture(joint_count: usize) -> GltfDocument {
    let nodes = (0..joint_count)
        .map(|index| DocumentNode {
            children: (index + 1 < joint_count)
                .then_some(vec![index + 1])
                .unwrap_or_default(),
            parent: if index > 0 { Some(index - 1) } else { None },
            mesh: (index == 0).then_some(0),
            skin: (index == 0).then_some(0),
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        })
        .collect();
    GltfDocument {
        buffers: Arc::new(Vec::new()),
        images: Arc::new(Vec::new()),
        nodes: Arc::new(nodes),
        meshes: Arc::new(vec![DocumentMesh {
            primitives: vec![DocumentPrimitive {
                mesh: None,
                weights: vec![JointWeights {
                    joints: [0, 1, 0, 0],
                    weights: [1.0, 0.0, 0.0, 0.0],
                }],
            }],
        }]),
        skins: Arc::new(vec![DocumentSkin {
            joints: (0..joint_count).collect(),
            inverse_bind_matrices: vec![[[1.0, 0.0, 0.0, 0.0]; 4]; joint_count],
        }]),
        animations: Arc::new(Vec::new()),
    }
}

fn valid_128() -> GltfDocument {
    fixture(128)
}

fn malformed_graph() -> GltfDocument {
    let mut document = fixture(2);
    Arc::make_mut(&mut document.nodes)[1].children.push(0);
    document
}

fn invalid_129() -> GltfDocument {
    fixture(129)
}

fn channel(values: AnimationValues, times: Vec<f32>) -> AnimationChannel {
    AnimationChannel {
        target_joint: 0,
        target: AnimationPath::Translation,
        interpolation: Interpolation::Linear,
        times: times.into(),
        values,
    }
}

#[test]
fn t7_gltf_document_fixture_valid_128_builds_typed_skeleton_without_truncation() {
    let document = valid_128();
    let report = document.validate().unwrap();
    let skeleton = SkeletonAsset::from_skin(&document, 0).unwrap();
    assert_eq!(skeleton.joints.len(), 128);
    assert!(report.diagnostics.is_empty());
}

#[test]
fn skeleton_rejects_inverse_bind_count_mismatch() {
    let mut document = fixture(2);
    Arc::make_mut(&mut document.skins)[0]
        .inverse_bind_matrices
        .pop();
    assert_eq!(document.validate(), Err(ValidationError::InverseBindCount));
}

#[test]
fn t7_gltf_document_fixture_malformed_graph_reports_cycle() {
    let document = malformed_graph();
    assert_eq!(document.validate(), Err(ValidationError::Cycle));
}

#[test]
fn t7_gltf_document_fixture_invalid_129_reports_static_fallback_without_truncating_skin() {
    let document = invalid_129();
    let report = document.validate().unwrap();
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(
        report.diagnostics[0],
        ValidationDiagnostic::UnsupportedJointCount {
            skin: 0,
            primitive: (0, 0),
            joint_count: 129,
        }
    );
    assert_eq!(document.skins[0].joints.len(), 129);
    assert!(matches!(
        SkeletonAsset::from_skin(&document, 0),
        Err(ValidationError::UnsupportedJointCount { count: 129 })
    ));
}

#[test]
fn weights_normalize_and_reject_zero_sum_or_invalid_values() {
    let normalized = JointWeights::validated([0, 1, 0, 0], [0.2, 0.2, 0.0, 0.0]).unwrap();
    assert_eq!(normalized.weights, [0.5, 0.5, 0.0, 0.0]);
    assert!(matches!(
        JointWeights::validated([0; 4], [0.0; 4]),
        Err(ValidationError::WeightSumTooSmall)
    ));
    assert!(matches!(
        JointWeights::validated([0; 4], [f32::NAN, 1.0, 0.0, 0.0]),
        Err(ValidationError::InvalidWeight)
    ));
}

#[test]
fn animation_channels_validate_typed_arity_and_strict_times() {
    let mut document = fixture(1);
    let animation = DocumentAnimation {
        channels: vec![channel(
            AnimationValues::Vec3(Arc::from([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]])),
            vec![0.0, 1.0],
        )],
    };
    Arc::make_mut(&mut document.animations).push(animation);
    assert!(document.validate().is_ok());
    Arc::make_mut(&mut document.animations)[0].channels[0].values =
        AnimationValues::Vec4(Arc::from([[1.0; 4], [1.0; 4]]));
    assert_eq!(document.validate(), Err(ValidationError::ChannelArity));
}

#[test]
fn inverse_bind_matrices_must_be_finite() {
    let mut document = fixture(1);
    Arc::make_mut(&mut document.skins)[0].inverse_bind_matrices[0][0][0] = f32::NAN;
    assert_eq!(
        document.validate(),
        Err(ValidationError::InvalidInverseBind)
    );
}

#[test]
fn animation_channels_reject_non_finite_or_non_increasing_times() {
    let mut document = fixture(1);
    Arc::make_mut(&mut document.animations).push(DocumentAnimation {
        channels: vec![channel(
            AnimationValues::Vec3(Arc::from([[0.0; 3], [1.0; 3]])),
            vec![0.5, 0.5],
        )],
    });
    assert_eq!(
        document.validate(),
        Err(ValidationError::InvalidChannelData)
    );
    Arc::make_mut(&mut document.animations)[0].channels[0].times = Arc::from([0.0, f32::INFINITY]);
    assert_eq!(
        document.validate(),
        Err(ValidationError::InvalidChannelData)
    );
}
