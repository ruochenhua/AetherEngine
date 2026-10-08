use super::*;
use crate::asset::gltf_document::{
    AnimationChannel, AnimationPath, AnimationValues, DocumentNode, DocumentSkin, Interpolation,
};

fn document() -> (GltfDocument, SkeletonAsset) {
    let document = GltfDocument {
        nodes: Arc::new(vec![DocumentNode::default()]),
        skins: Arc::new(vec![DocumentSkin {
            joints: vec![0],
            inverse_bind_matrices: vec![Mat4::IDENTITY.to_cols_array_2d()],
        }]),
        ..GltfDocument::default()
    };
    let skeleton = SkeletonAsset::from_skin(&document, 0).unwrap();
    (document, skeleton)
}

#[test]
fn linear_channels_sample_vec3_and_slerp_quaternions() {
    let (document, skeleton) = document();
    let clip = AnimationClipAsset {
        channels: vec![
            AnimationChannel {
                target_joint: 0,
                target: AnimationPath::Translation,
                interpolation: Interpolation::Linear,
                times: Arc::from([0.0, 1.0]),
                values: AnimationValues::Vec3(Arc::from([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]])),
            },
            AnimationChannel {
                target_joint: 0,
                target: AnimationPath::Rotation,
                interpolation: Interpolation::Linear,
                times: Arc::from([0.0, 1.0]),
                values: AnimationValues::Vec4(Arc::from([
                    [0.0, 0.0, 0.0, 1.0],
                    [0.0, 0.0, 1.0, 0.0],
                ])),
            },
        ],
        duration: 1.0,
    };
    let sample = sample_animator(&document, &skeleton, Some(&clip), 0.5).unwrap();
    assert!(!sample.bind_pose_fallback);
    assert!((sample.locals[0].translation.x - 0.5).abs() < 1.0e-6);
    let expected = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    assert!(sample.locals[0].rotation.dot(expected).abs() > 0.9999);
}

#[test]
fn missing_or_zero_duration_clip_uses_bind_pose() {
    let (document, skeleton) = document();
    assert!(
        sample_animator(&document, &skeleton, None, 0.25)
            .unwrap()
            .bind_pose_fallback
    );
    assert!(
        sample_animator(
            &document,
            &skeleton,
            Some(&AnimationClipAsset::default()),
            0.25
        )
        .unwrap()
        .bind_pose_fallback
    );
}

#[test]
fn cubic_spline_is_rejected_and_step_holds_previous_key() {
    let (document, skeleton) = document();
    let step = AnimationClipAsset {
        channels: vec![AnimationChannel {
            target_joint: 0,
            target: AnimationPath::Scale,
            interpolation: Interpolation::Step,
            times: Arc::from([0.0, 1.0]),
            values: AnimationValues::Vec3(Arc::from([[1.0, 1.0, 1.0], [2.0, 2.0, 2.0]])),
        }],
        duration: 1.0,
    };
    let sample = sample_animator(&document, &skeleton, Some(&step), 0.75).unwrap();
    assert_eq!(sample.locals[0].scale, Vec3::ONE);
    let mut cubic = step;
    cubic.channels[0].interpolation = Interpolation::CubicSpline;
    assert_eq!(
        sample_animator(&document, &skeleton, Some(&cubic), 0.75),
        Err(AnimationError::UnsupportedInterpolation)
    );
}

#[test]
fn frozen_pose_hashes_match_all_seek_variants() {
    let expected = [
        (
            "seek_0",
            "0.000000000",
            "47985fe07dd05522032afb8b1789c437c7ddc1a8f5bf86edb1ce129dcd3b19b9",
        ),
        (
            "seek_0p25",
            "0.250000005",
            "8e84f0b3ca1b452d9d9978b093f950f76a205b0aff5fd80977e089978784fae5",
        ),
        (
            "seek_0p50",
            "0.500000010",
            "d6e118c8e5e8d72144a028dc7ed3c4c40234ffa03fa728f3472206455eca3703",
        ),
        (
            "seek_0p75",
            "0.750000015",
            "f6996f06d3971904fedc85d96f173b2b7b65c82e39e6d0b1908730fb5cebe1e6",
        ),
    ];
    let nodes = (0..128)
        .map(|index| DocumentNode {
            children: if index + 1 < 128 {
                vec![index + 1]
            } else {
                Vec::new()
            },
            parent: (index > 0).then(|| index - 1),
            mesh: None,
            skin: None,
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        })
        .collect::<Vec<_>>();
    let document = GltfDocument {
        nodes: Arc::new(nodes),
        skins: Arc::new(vec![DocumentSkin {
            joints: (0..128).collect(),
            inverse_bind_matrices: vec![Mat4::IDENTITY.to_cols_array_2d(); 128],
        }]),
        ..GltfDocument::default()
    };
    let skeleton = SkeletonAsset::from_skin(&document, 0).unwrap();
    let clip = AnimationClipAsset {
        channels: vec![AnimationChannel {
            target_joint: 0,
            target: AnimationPath::Translation,
            interpolation: Interpolation::Linear,
            times: Arc::from([0.0, 1.0]),
            values: AnimationValues::Vec3(Arc::from([[0.0; 3], [1.0, 0.0, 0.0]])),
        }],
        duration: 1.0,
    };
    for (variant, time, hash) in expected {
        let pose =
            sample_animator(&document, &skeleton, Some(&clip), time.parse().unwrap()).unwrap();
        assert_eq!(
            pose_state_hash(variant, time, &pose).unwrap(),
            hash,
            "variant {variant}"
        );
    }
}

#[test]
fn animator_seek_pause_speed_and_loop_are_deterministic() {
    let mut runtime = AnimatorRuntime::default();
    runtime.states.insert(
        7,
        AnimatorState {
            time: 0.25,
            speed: 2.0,
            looping: true,
            playing: false,
            ..AnimatorState::default()
        },
    );
    runtime.advance(0.5).unwrap();
    assert_eq!(runtime.states[&7].time, 0.25);
    runtime.states.get_mut(&7).unwrap().playing = true;
    runtime.advance(0.5).unwrap();
    assert_eq!(runtime.states[&7].time, 1.25);
    runtime.seek(7, 0.75).unwrap();
    assert_eq!(runtime.states[&7].time, 0.75);
}
