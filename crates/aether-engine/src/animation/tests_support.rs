use super::{
    extract_animation_frame, AnimationAssets, AnimationCameraFrame, AnimationContext,
    AnimationFrame, AnimationPrimitive, AnimatorRuntime, AnimatorState, MeshRef, SceneSnapshot,
};
use crate::asset::gltf_document::{
    AnimationChannel, AnimationClipAsset, AnimationPath, AnimationValues, DocumentMesh,
    DocumentNode, DocumentPrimitive, DocumentSkin, GltfDocument, Interpolation, JointWeights,
    SkeletonAsset,
};
use crate::asset::store::{canonicalize, AssetId, AssetKind};
use crate::asset::{mesh::CpuMesh, Handle};
use crate::math::{Aabb, Mat4};
use std::path::Path;
use std::sync::Arc;

struct TestAssets {
    document: Arc<GltfDocument>,
    skeleton: Arc<SkeletonAsset>,
    clip: Arc<AnimationClipAsset>,
}

impl AnimationAssets for TestAssets {
    fn document(
        &self,
        _handle: Handle<GltfDocument>,
    ) -> Result<Arc<GltfDocument>, super::AnimationError> {
        Ok(self.document.clone())
    }

    fn skeleton(
        &self,
        _handle: Handle<SkeletonAsset>,
    ) -> Result<Arc<SkeletonAsset>, super::AnimationError> {
        Ok(self.skeleton.clone())
    }

    fn clip(
        &self,
        _handle: Handle<AnimationClipAsset>,
    ) -> Result<Arc<AnimationClipAsset>, super::AnimationError> {
        Ok(self.clip.clone())
    }
}

/// Build a real skinned AnimationFrame through the production extractor for renderer tests.
pub(crate) fn animation_frame_with_bounds(bounds: Aabb) -> AnimationFrame {
    let static_mesh = CpuMesh {
        positions: vec![bounds.min.to_array(), bounds.max.to_array()],
        normals: vec![[0.0, 1.0, 0.0]; 2],
        uvs: vec![[0.0, 0.0]; 2],
        tangents: Vec::new(),
        indices: Vec::new(),
        submeshes: Vec::new(),
    };
    let document = Arc::new(GltfDocument {
        nodes: Arc::new(vec![DocumentNode {
            mesh: Some(0),
            skin: Some(0),
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
            children: Vec::new(),
            parent: None,
        }]),
        skins: Arc::new(vec![DocumentSkin {
            joints: vec![0],
            inverse_bind_matrices: vec![Mat4::IDENTITY.to_cols_array_2d()],
        }]),
        meshes: Arc::new(vec![DocumentMesh {
            primitives: vec![DocumentPrimitive {
                mesh: Some(static_mesh.clone()),
                weights: vec![
                    JointWeights::validated([0; 4], [1.0, 0.0, 0.0, 0.0])
                        .expect("fixture weights should validate");
                    2
                ],
            }],
        }]),
        ..GltfDocument::default()
    });
    let skeleton = Arc::new(
        SkeletonAsset::from_skin_with_source(
            &document,
            0,
            AssetId {
                kind: AssetKind::Skeleton,
                path: canonicalize(Path::new("."), Path::new("tests/fixtures/rig.gltf"))
                    .expect("fixture skeleton id is canonical"),
            },
        )
        .expect("fixture skeleton should validate"),
    );
    let clip = Arc::new(AnimationClipAsset {
        channels: vec![AnimationChannel {
            target_joint: 0,
            target: AnimationPath::Translation,
            interpolation: Interpolation::Linear,
            times: Arc::from([0.0, 1.0]),
            values: AnimationValues::Vec3(Arc::from([[0.0; 3], [0.0; 3]])),
        }],
        duration: 1.0,
    });
    let assets = TestAssets {
        document,
        skeleton,
        clip,
    };
    let primitives = [AnimationPrimitive {
        entity_bits: 1,
        primitive_index: 0,
        node_index: 0,
        mesh: MeshRef {
            asset: AssetId {
                kind: AssetKind::CpuMesh,
                path: canonicalize(Path::new("."), Path::new("tests/fixtures/mesh.gltf"))
                    .expect("fixture mesh id is canonical"),
            },
            generation: 1,
        },
        document: Handle::new(1),
        skeleton: Some(Handle::new(2)),
        static_mesh: Arc::new(static_mesh),
        world_transform: Mat4::IDENTITY,
    }];
    let snapshot = SceneSnapshot {
        generation: 1,
        frame_id: 1,
    };
    let camera = AnimationCameraFrame {
        view_projection: Mat4::IDENTITY,
    };
    let context = AnimationContext {
        snapshot: &snapshot,
        assets: &assets,
        primitives: &primitives,
        camera: &camera,
        scene_generation: 1,
        frame_id: 1,
    };
    let mut runtime = AnimatorRuntime {
        states: Default::default(),
        generation: 1,
    };
    runtime.states.insert(
        1,
        AnimatorState {
            clip: Some(Handle::new(3)),
            time: 0.0,
            ..AnimatorState::default()
        },
    );
    extract_animation_frame(&runtime, &context).expect("fixture animation should extract")
}
