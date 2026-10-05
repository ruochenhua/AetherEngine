use super::*;
use crate::asset::{
    id::{AssetId, AssetKind, CanonicalPath},
    loaders::gltf::{load_document, AnimationClipAsset, SkeletonAsset},
};
use std::path::{Path, PathBuf};

fn fixture() -> (AnimationClipAsset, SkeletonAsset) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/t7_animated_actor.gltf");
    let document = load_document(&path).unwrap();
    let relative = Path::new("tests/fixtures/t7_animated_actor.gltf");
    let document_id = AssetId::new(
        AssetKind::GltfDocument,
        CanonicalPath::new(Path::new("."), relative).unwrap(),
    );
    let skeleton = SkeletonAsset::from_document(document_id.clone(), &document, 0).unwrap();
    let clip = AnimationClipAsset::from_document(document_id, &document, 0, &skeleton).unwrap();
    (clip, skeleton)
}

#[test]
fn seek_samples_bind_and_interpolated_arm_rotations_from_the_owned_gltf() {
    let (clip, skeleton) = fixture();
    assert_eq!(skeleton.joints.len(), 3);
    assert_eq!(clip.channels.len(), 2);

    let bind = sample_animation_pose(&clip, &skeleton, 0.0).unwrap();
    assert_eq!(bind[1].rotation, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(bind[2].rotation, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(bind[1].translation, [-0.36, 1.76, 0.0]);
    assert_eq!(bind[2].translation, [0.36, 1.76, 0.0]);

    let halfway = sample_animation_pose(&clip, &skeleton, 0.375).unwrap();
    assert_ne!(halfway[1].rotation, bind[1].rotation);
    assert_ne!(halfway[2].rotation, bind[2].rotation);
    let end = sample_animation_pose(&clip, &skeleton, 0.75).unwrap();
    assert_ne!(end[1].rotation, halfway[1].rotation);
    assert_ne!(end[2].rotation, halfway[2].rotation);
}

#[test]
fn animator_state_supports_seek_pause_speed_loop_and_reverse() {
    let mut state = AnimatorState::new(false);
    assert_eq!(state.seek(0.25, 0.75).unwrap(), 0.25);
    state.play();
    assert_eq!(state.advance(0.25, 0.75).unwrap(), 0.5);
    state.pause();
    assert_eq!(state.advance(0.1, 0.75).unwrap(), 0.5);

    state.set_speed(2.0).unwrap();
    state.play();
    assert_eq!(state.advance(0.1, 0.75).unwrap(), 0.7);
    assert_eq!(state.advance(0.1, 0.75).unwrap(), 0.75);
    assert!(!state.is_playing());

    state.set_speed(-1.0).unwrap();
    state.play();
    assert_eq!(state.advance(1.0, 0.75).unwrap(), 0.0);
    assert!(!state.is_playing());

    state.set_looping(true);
    state.set_speed(1.0).unwrap();
    state.seek(0.5, 0.75).unwrap();
    state.play();
    assert_eq!(state.advance(0.5, 0.75).unwrap(), 0.25);
    assert!(state.is_playing());
    assert!(state.set_speed(f32::NAN).is_err());
}

#[test]
fn t7_pose_hashes_are_stable_for_the_four_review_times() {
    let (clip, skeleton) = fixture();
    let hashes: Vec<_> = [0.0, 0.25, 0.5, 0.75]
        .into_iter()
        .map(|time| {
            let pose = sample_animation_pose(&clip, &skeleton, time).unwrap();
            to_hex(&pose_state_hash(&pose))
        })
        .collect();
    assert_eq!(
        hashes,
        [
            "9ce3b289358a582b26c1c18e578db92d7d47174ae20d8fe1c5753ba59c49b39c",
            "775728d387d96cce57f009f1f202afcb1e23cec4d225f17f3592034d43a72b56",
            "872377907d96369317d3c55eb4e1d1b9472b297fc10f97de15226ac31fa265cc",
            "1019cdd84b869ed8821d575177b84ab41824de411b3d1a0192f327a4c828ce88",
        ]
    );
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
