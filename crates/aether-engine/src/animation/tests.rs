use super::*;
use crate::asset::id::{AssetId, AssetKind, CanonicalPath};
use crate::asset::loaders::gltf::{
    GltfDocumentAsset, GltfMesh, GltfNode, GltfPrimitive, GltfSkin, SkeletonAsset,
};
use crate::math::{Mat4, Vec3};
use std::path::Path;
use std::sync::Arc;

struct Assets {
    document_id: AssetId,
    skeleton_id: AssetId,
    document: Arc<GltfDocumentAsset>,
    skeleton: Arc<SkeletonAsset>,
}

impl AnimationAssets for Assets {
    fn document(&self, id: &AssetId) -> Option<Arc<GltfDocumentAsset>> {
        (id == &self.document_id).then(|| Arc::clone(&self.document))
    }

    fn skeleton(&self, id: &AssetId) -> Option<Arc<SkeletonAsset>> {
        (id == &self.skeleton_id).then(|| Arc::clone(&self.skeleton))
    }
}

fn asset_id(kind: AssetKind, path: &str) -> AssetId {
    AssetId::new(
        kind,
        CanonicalPath::new(Path::new("."), Path::new(path)).unwrap(),
    )
}

fn fixture() -> (Assets, AnimationPrimitiveMapping) {
    let document_id = asset_id(AssetKind::GltfDocument, "animation/t7.gltf");
    let skeleton_id = asset_id(AssetKind::Skeleton, "animation/t7.skeleton");
    let root = Arc::new(GltfNode {
        name: Some("RootJoint".into()),
        parent: None,
        children: Arc::from([]),
        mesh: Some(0),
        skin: Some(0),
        local_matrix: Mat4::IDENTITY.to_cols_array_2d(),
        translation: [0.0, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0, 1.0, 1.0],
    });
    let primitive = GltfPrimitive {
        positions: Arc::from([[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]]),
        normals: None,
        tangents: None,
        tex_coords: None,
        indices: Arc::from([0, 1]),
        joints: Some(Arc::from([[0, 0, 0, 0], [0, 0, 0, 0]])),
        weights: Some(Arc::from([[1.0, 0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]])),
    };
    let document = Arc::new(GltfDocumentAsset {
        buffers: Vec::new(),
        images: Vec::new(),
        nodes: vec![root],
        meshes: vec![Arc::new(GltfMesh {
            name: Some("T7Fixture".into()),
            primitives: Arc::from([primitive]),
        })],
        skins: vec![Arc::new(GltfSkin {
            name: Some("T7Fixture".into()),
            skeleton: Some(0),
            joints: Arc::from([0]),
            inverse_bind: Arc::from([Mat4::IDENTITY]),
        })],
        animations: Vec::new(),
    });
    let skeleton =
        Arc::new(SkeletonAsset::from_document(document_id.clone(), &document, 0).unwrap());
    let mapping = AnimationPrimitiveMapping {
        entity_id: 7,
        mesh_index: 0,
        primitive_index: 0,
        joint_count: 1,
        document: document_id.clone(),
        skeleton: Some(skeleton_id.clone()),
        world_transform: Mat4::IDENTITY.to_cols_array_2d(),
    };
    (
        Assets {
            document_id,
            skeleton_id,
            document,
            skeleton,
        },
        mapping,
    )
}

fn pose_x(x: f32) -> LocalPose {
    LocalPose {
        translation: [x, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0, 1.0, 1.0],
    }
}

#[test]
fn gpu_palette_has_fixed_128_matrix_column_major_abi() {
    let mut matrices = vec![Mat4::IDENTITY; 128];
    matrices[0] = Mat4::from_translation(Vec3::new(3.0, 4.0, 5.0));
    let palette = JointPalette { matrices };
    let gpu = JointPaletteGpu::from_palette(&palette).unwrap();
    assert_eq!(gpu.joint_count, 128);
    assert_eq!(gpu.matrices[0][3], [3.0, 4.0, 5.0, 1.0]);
    assert_eq!(gpu.matrices[127], Mat4::IDENTITY.to_cols_array_2d());
    assert_eq!(std::mem::size_of::<JointPaletteGpu>(), 128 * 64 + 16);
}

#[test]
fn extracted_palette_and_bounds_share_the_current_pose() {
    let (assets, mapping) = fixture();
    let primitives = AnimationPrimitiveSnapshot {
        primitives: Arc::from([mapping]),
    };
    let context = AnimationContext {
        snapshot: SceneSnapshot { generation: 4 },
        assets: &assets,
        primitives: &primitives,
        camera: AnimationCameraFrame {
            view_projection: Mat4::IDENTITY.to_cols_array_2d(),
        },
        scene_generation: 4,
        frame_id: 9,
    };
    let mut runtime = AnimatorRuntime::new(4);
    runtime.set_pose(7, vec![pose_x(2.0)]);
    let frame = extract_animation_frame(&mut runtime, &context).unwrap();

    assert_eq!(frame.scene_generation, 4);
    assert_eq!(frame.frame_id, 9);
    assert_eq!(frame.items.len(), 1);
    match &frame.items[0].result {
        AnimationItemResult::Skinned {
            palette,
            world_bounds,
        } => {
            assert_eq!(palette.joint_count, 1);
            assert_eq!(palette.matrices[0][3], [2.0, 0.0, 0.0, 1.0]);
            assert_eq!(world_bounds.min, Vec3::new(1.0, 0.0, 0.0));
            assert_eq!(world_bounds.max, Vec3::new(3.0, 0.0, 0.0));
        }
        AnimationItemResult::StaticFallback(reason) => panic!("unexpected fallback: {reason:?}"),
    }
    assert!(frame.diagnostics.is_empty());
}

#[test]
fn item_fallback_does_not_drop_other_primitive_outputs() {
    let (assets, mapping) = fixture();
    let mut missing_pose = mapping.clone();
    missing_pose.entity_id = 8;
    let primitives = AnimationPrimitiveSnapshot {
        primitives: Arc::from([mapping, missing_pose]),
    };
    let context = AnimationContext {
        snapshot: SceneSnapshot { generation: 2 },
        assets: &assets,
        primitives: &primitives,
        camera: AnimationCameraFrame {
            view_projection: Mat4::IDENTITY.to_cols_array_2d(),
        },
        scene_generation: 2,
        frame_id: 1,
    };
    let mut runtime = AnimatorRuntime::new(2);
    runtime.set_pose(7, vec![pose_x(0.5)]);
    let frame = extract_animation_frame(&mut runtime, &context).unwrap();

    assert!(matches!(
        frame.items[0].result,
        AnimationItemResult::Skinned { .. }
    ));
    assert!(matches!(
        frame.items[1].result,
        AnimationItemResult::StaticFallback(StaticFallbackReason::MissingPose)
    ));
    assert_eq!(frame.diagnostics.len(), 1);
    assert_eq!(frame.diagnostics[0].entity_id, 8);
}

#[test]
fn stale_generation_rejects_publication_without_advancing_frame_id() {
    let (assets, mapping) = fixture();
    let primitives = AnimationPrimitiveSnapshot {
        primitives: Arc::from([mapping]),
    };
    let context = AnimationContext {
        snapshot: SceneSnapshot { generation: 3 },
        assets: &assets,
        primitives: &primitives,
        camera: AnimationCameraFrame {
            view_projection: Mat4::IDENTITY.to_cols_array_2d(),
        },
        scene_generation: 3,
        frame_id: 12,
    };
    let mut runtime = AnimatorRuntime::new(2);
    let error = extract_animation_frame(&mut runtime, &context).unwrap_err();
    assert!(matches!(error, AnimationError::StaleGeneration { .. }));
    assert_eq!(runtime.last_published_frame, None);
}

#[test]
fn primitive_bounds_reject_out_of_range_joint_indices() {
    let (assets, mapping) = fixture();
    let mut primitive = assets.document.meshes[0].primitives[0].clone();
    primitive.joints = Some(Arc::from([[1, 0, 0, 0], [0, 0, 0, 0]]));
    let palette = JointPalette {
        matrices: vec![Mat4::IDENTITY],
    };
    assert_eq!(
        skinned_world_bounds(
            &primitive,
            &palette,
            Mat4::from_cols_array_2d(&mapping.world_transform)
        ),
        Err(StaticFallbackReason::JointOutOfRange)
    );
}

#[test]
fn palette_includes_non_joint_ancestor_transform() {
    let (assets, _) = fixture();
    let mut document = (*assets.document).clone();
    let group = Arc::new(GltfNode {
        name: Some("Intermediate".into()),
        parent: None,
        children: Arc::from([1]),
        mesh: None,
        skin: None,
        local_matrix: Mat4::from_translation(Vec3::X * 3.0).to_cols_array_2d(),
        translation: [3.0, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0, 1.0, 1.0],
    });
    let joint = Arc::new(GltfNode {
        name: Some("RootJoint".into()),
        parent: Some(0),
        children: Arc::from([]),
        mesh: Some(0),
        skin: Some(0),
        local_matrix: Mat4::from_translation(Vec3::X * 2.0).to_cols_array_2d(),
        translation: [2.0, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0, 1.0, 1.0],
    });
    document.nodes = vec![group, joint];
    document.skins[0] = Arc::new(crate::asset::loaders::gltf::GltfSkin {
        name: Some("T7Fixture".into()),
        skeleton: Some(1),
        joints: Arc::from([1]),
        inverse_bind: Arc::from([Mat4::from_translation(Vec3::X)]),
    });
    let skeleton = SkeletonAsset::from_document(assets.document_id, &document, 0).unwrap();
    let rotated_pose = LocalPose {
        translation: [0.0; 3],
        rotation: [
            0.0,
            0.0,
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
        ],
        scale: [1.0; 3],
    };
    let palette = build_joint_palette(&document, &skeleton, &[rotated_pose]).unwrap();
    assert!(palette.matrices[0]
        .w_axis
        .truncate()
        .abs_diff_eq(Vec3::new(3.0, 1.0, 0.0), 1.0e-5));
}

#[test]
fn storage_layout_binds_and_uploads_a_palette() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("need adapter");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("need device");
    let storage = JointPaletteStorage::new(&device);
    let _bind_group = storage.bind_group(&device);
    let palette = JointPaletteGpu::from_palette(&JointPalette {
        matrices: vec![Mat4::IDENTITY; 128],
    })
    .unwrap();
    storage.upload(&queue, &palette).unwrap();
    assert_eq!(std::mem::size_of_val(&palette), 128 * 64 + 16);
}

#[test]
fn non_finite_pose_falls_back_only_for_the_affected_item() {
    let (assets, mapping) = fixture();
    let primitives = AnimationPrimitiveSnapshot {
        primitives: Arc::from([mapping]),
    };
    let context = AnimationContext {
        snapshot: SceneSnapshot { generation: 5 },
        assets: &assets,
        primitives: &primitives,
        camera: AnimationCameraFrame {
            view_projection: Mat4::IDENTITY.to_cols_array_2d(),
        },
        scene_generation: 5,
        frame_id: 1,
    };
    let mut runtime = AnimatorRuntime::new(5);
    runtime.set_pose(
        7,
        vec![LocalPose {
            translation: [f32::NAN, 0.0, 0.0],
            ..pose_x(0.0)
        }],
    );
    let frame = extract_animation_frame(&mut runtime, &context).unwrap();
    assert!(matches!(
        frame.items[0].result,
        AnimationItemResult::StaticFallback(StaticFallbackReason::InvalidNumericData)
    ));
    assert_eq!(frame.diagnostics.len(), 1);
}

#[test]
fn unsupported_129_joint_item_falls_back_without_a_skeleton_asset() {
    let (assets, mut mapping) = fixture();
    mapping.joint_count = 129;
    mapping.skeleton = None;
    let primitives = AnimationPrimitiveSnapshot {
        primitives: Arc::from([mapping]),
    };
    let context = AnimationContext {
        snapshot: SceneSnapshot { generation: 6 },
        assets: &assets,
        primitives: &primitives,
        camera: AnimationCameraFrame {
            view_projection: Mat4::IDENTITY.to_cols_array_2d(),
        },
        scene_generation: 6,
        frame_id: 1,
    };
    let mut runtime = AnimatorRuntime::new(6);
    let frame = extract_animation_frame(&mut runtime, &context).unwrap();
    assert!(matches!(
        frame.items[0].result,
        AnimationItemResult::StaticFallback(StaticFallbackReason::JointLimit)
    ));
    assert_eq!(frame.items[0].entity_id, 7);
}
