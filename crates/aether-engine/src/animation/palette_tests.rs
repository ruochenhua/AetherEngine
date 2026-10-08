use super::*;
use crate::asset::gltf_document::{DocumentNode, SkeletonAsset};
use crate::asset::store::{canonicalize, AssetKind};
use glam::{Quat, Vec3};
use std::path::Path;
use std::sync::Arc;

fn source_id() -> AssetId {
    AssetId {
        kind: AssetKind::Skeleton,
        path: canonicalize(Path::new("."), Path::new("actor/rig.gltf")).unwrap(),
    }
}

fn one_joint_document() -> (GltfDocument, SkeletonAsset) {
    let document = GltfDocument {
        nodes: Arc::new(vec![DocumentNode {
            translation: [1.0, 0.0, 0.0],
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array(),
            scale: [1.0; 3],
            ..DocumentNode::default()
        }]),
        ..GltfDocument::default()
    };
    let skeleton = SkeletonAsset {
        source: Some(source_id()),
        joints: vec![0],
        inverse_bind_matrices: vec![
            Mat4::from_translation(Vec3::new(0.0, 2.0, 0.0)).to_cols_array_2d()
        ],
    };
    (document, skeleton)
}

#[test]
fn palette_uses_column_major_inverse_bind_times_global_joint() {
    let (document, skeleton) = one_joint_document();
    let pose = PoseSample {
        locals: Arc::from(
            document
                .nodes
                .iter()
                .map(LocalPose::from)
                .collect::<Vec<_>>(),
        ),
        bind_pose_fallback: false,
    };
    let bind_bounds = Aabb::new(Vec3::splat(-0.5), Vec3::splat(0.5));
    let palette =
        build_joint_palette(&document, &skeleton, &pose, bind_bounds, Mat4::IDENTITY, 9).unwrap();
    let inverse = Mat4::from_cols_array_2d(&skeleton.inverse_bind_matrices[0]);
    let global = Mat4::from_scale_rotation_translation(
        Vec3::ONE,
        Quat::from_array(document.nodes[0].rotation),
        Vec3::from_array(document.nodes[0].translation),
    );
    assert!(palette.matrices[0].abs_diff_eq(inverse * global, 1.0e-6));
    assert_eq!(palette.generation, 9);
    assert_eq!(palette.skeleton, source_id());
}

#[test]
fn palette_supports_exactly_128_matrices_and_conservative_shared_bounds() {
    let count = 128;
    let nodes = (0..count)
        .map(|index| DocumentNode {
            parent: if index > 0 { Some(index - 1) } else { None },
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
            ..DocumentNode::default()
        })
        .collect::<Vec<_>>();
    let document = GltfDocument {
        nodes: Arc::new(nodes),
        ..GltfDocument::default()
    };
    let skeleton = SkeletonAsset {
        source: Some(source_id()),
        joints: (0..count).collect(),
        inverse_bind_matrices: vec![Mat4::IDENTITY.to_cols_array_2d(); count],
    };
    let pose = PoseSample {
        locals: Arc::from(
            document
                .nodes
                .iter()
                .map(LocalPose::from)
                .collect::<Vec<_>>(),
        ),
        bind_pose_fallback: false,
    };
    let bounds = Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0));
    let palette = build_joint_palette(
        &document,
        &skeleton,
        &pose,
        bounds,
        Mat4::from_translation(Vec3::new(3.0, 0.0, 0.0)),
        1,
    )
    .unwrap();
    assert_eq!(palette.matrices.len(), 128);
    assert_eq!(
        palette.matrices.len() * std::mem::size_of::<Mat4>(),
        128 * 64
    );
    assert_eq!(
        palette.bounds,
        bounds.transform(Mat4::from_translation(Vec3::new(3.0, 0.0, 0.0)))
    );
}
