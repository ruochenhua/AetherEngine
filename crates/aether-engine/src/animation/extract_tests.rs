use super::*;
use crate::animation::{AnimationCameraFrame, AnimatorState};
use crate::asset::gltf_document::{
    AnimationChannel, AnimationPath, AnimationValues, DocumentMesh, DocumentNode,
    DocumentPrimitive, DocumentSkin, Interpolation, JointWeights,
};
use crate::asset::store::{canonicalize, AssetKind};
use glam::Vec3;
use std::path::Path;

#[derive(Default)]
struct Assets {
    document: Arc<GltfDocument>,
    skeleton: Option<Arc<SkeletonAsset>>,
    clip: Option<Arc<AnimationClipAsset>>,
}

impl AnimationAssets for Assets {
    fn document(&self, _handle: Handle<GltfDocument>) -> Result<Arc<GltfDocument>, AnimationError> {
        Ok(self.document.clone())
    }

    fn skeleton(
        &self,
        _handle: Handle<SkeletonAsset>,
    ) -> Result<Arc<SkeletonAsset>, AnimationError> {
        self.skeleton.clone().ok_or(AnimationError::MissingAsset)
    }

    fn clip(
        &self,
        _handle: Handle<AnimationClipAsset>,
    ) -> Result<Arc<AnimationClipAsset>, AnimationError> {
        self.clip.clone().ok_or(AnimationError::MissingAsset)
    }
}

fn source(kind: AssetKind, path: &str) -> AssetId {
    AssetId {
        kind,
        path: canonicalize(Path::new("."), Path::new(path)).unwrap(),
    }
}

fn fixture(joint_count: usize) -> Assets {
    let nodes = (0..joint_count)
        .map(|index| DocumentNode {
            children: if index + 1 < joint_count {
                vec![index + 1]
            } else {
                Vec::new()
            },
            parent: (index > 0).then(|| index - 1),
            mesh: (index == 0).then_some(0),
            skin: (index == 0).then_some(0),
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        })
        .collect::<Vec<_>>();
    let document = Arc::new(GltfDocument {
        nodes: Arc::new(nodes),
        meshes: Arc::new(vec![DocumentMesh {
            primitives: vec![DocumentPrimitive {
                mesh: Some(crate::asset::mesh::CpuMesh::cube()),
                weights: vec![JointWeights::validated([0; 4], [1.0, 0.0, 0.0, 0.0]).unwrap(); 24],
            }],
        }]),
        skins: Arc::new(vec![DocumentSkin {
            joints: (0..joint_count).collect(),
            inverse_bind_matrices: vec![glam::Mat4::IDENTITY.to_cols_array_2d(); joint_count],
        }]),
        ..GltfDocument::default()
    });
    let skeleton = (joint_count <= 128)
        .then(|| {
            SkeletonAsset::from_skin_with_source(
                &document,
                0,
                source(AssetKind::Skeleton, "actor/rig.gltf"),
            )
            .unwrap()
        })
        .map(Arc::new);
    let clip = AnimationClipAsset {
        channels: vec![AnimationChannel {
            target_joint: 0,
            target: AnimationPath::Translation,
            interpolation: Interpolation::Linear,
            times: Arc::from([0.0, 1.0]),
            values: AnimationValues::Vec3(Arc::from([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]])),
        }],
        duration: 1.0,
    };
    Assets {
        document,
        skeleton,
        clip: Some(Arc::new(clip)),
    }
}

fn primitive(skeleton: Option<Handle<SkeletonAsset>>) -> AnimationPrimitive {
    AnimationPrimitive {
        entity_bits: 7,
        primitive_index: 0,
        node_index: 0,
        mesh: MeshRef {
            asset: source(AssetKind::CpuMesh, "actor/body.mesh"),
            generation: 3,
        },
        document: Handle::new(1),
        skeleton,
        static_mesh: Arc::new(crate::asset::mesh::CpuMesh::cube()),
        world_transform: glam::Mat4::from_translation(Vec3::new(2.0, 0.0, 0.0)),
    }
}

fn context<'a>(
    assets: &'a Assets,
    primitive: &'a [AnimationPrimitive],
    snapshot: &'a SceneSnapshot,
    camera: &'a AnimationCameraFrame,
) -> AnimationContext<'a> {
    AnimationContext {
        snapshot,
        assets,
        primitives: primitive,
        camera,
        scene_generation: 4,
        frame_id: 12,
    }
}

#[test]
fn extracted_frame_publishes_one_palette_and_shared_bounds() {
    let assets = fixture(128);
    let primitive = [primitive(Some(Handle::new(2)))];
    let snapshot = SceneSnapshot {
        generation: 4,
        frame_id: 12,
    };
    let camera = AnimationCameraFrame {
        view_projection: glam::Mat4::IDENTITY,
    };
    let context = context(&assets, &primitive, &snapshot, &camera);
    let mut runtime = AnimatorRuntime {
        states: Default::default(),
        generation: 4,
    };
    runtime.states.insert(
        7,
        AnimatorState {
            clip: Some(Handle::new(3)),
            time: 0.5,
            ..AnimatorState::default()
        },
    );
    let frame = extract_animation_frame(&runtime, &context).unwrap();
    assert_eq!((frame.frame_id, frame.scene_generation), (12, 4));
    assert!(frame.diagnostics.is_empty());
    let AnimationRenderItem::Skinned(item) = &frame.items[0] else {
        panic!("expected a skinned item")
    };
    assert_eq!(item.palette.matrices.len(), 128);
    assert_eq!(item.bounds, item.palette.bounds);
    assert_eq!(item.source_mesh.positions.len(), item.weights.len());
    assert_eq!(item.weights[0].weights[0], 1.0);
}

#[test]
fn oversized_skin_falls_back_only_for_its_primitive() {
    let assets = fixture(129);
    let primitive = [primitive(None)];
    let snapshot = SceneSnapshot {
        generation: 4,
        frame_id: 12,
    };
    let camera = AnimationCameraFrame {
        view_projection: glam::Mat4::IDENTITY,
    };
    let context = context(&assets, &primitive, &snapshot, &camera);
    let runtime = AnimatorRuntime {
        states: Default::default(),
        generation: 4,
    };
    let frame = extract_animation_frame(&runtime, &context).unwrap();
    assert_eq!(frame.items.len(), 1);
    assert!(matches!(
        &frame.items[0],
        AnimationRenderItem::StaticFallback(StaticFallback {
            reason: AnimationError::Document(
                crate::asset::gltf_document::ValidationError::UnsupportedJointCount { count: 129 }
            ),
            ..
        })
    ));
    assert_eq!(frame.diagnostics[0].primitive_index, 0);
}

#[test]
fn stale_runtime_generation_rejects_the_whole_frame() {
    let assets = fixture(1);
    let primitive = [primitive(Some(Handle::new(2)))];
    let snapshot = SceneSnapshot {
        generation: 4,
        frame_id: 12,
    };
    let camera = AnimationCameraFrame {
        view_projection: glam::Mat4::IDENTITY,
    };
    let context = context(&assets, &primitive, &snapshot, &camera);
    let runtime = AnimatorRuntime {
        states: Default::default(),
        generation: 3,
    };
    assert!(matches!(
        extract_animation_frame(&runtime, &context),
        Err(AnimationError::StaleGeneration {
            expected: 4,
            actual: 3
        })
    ));
}
