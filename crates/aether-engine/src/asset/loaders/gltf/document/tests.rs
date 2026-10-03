use super::*;
use crate::asset::{Asset, AssetId, AssetKind};
use glam::Mat4;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(1);

struct Fixture {
    root: PathBuf,
    path: PathBuf,
}

impl Fixture {
    fn new(
        joint_count: usize,
        weight: [f32; 4],
        cycle: bool,
        times: [f32; 2],
        inverse_bind_count: Option<usize>,
    ) -> Self {
        let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("aether_t7_gltf_{}_{}", std::process::id(), id));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("actor.gltf");

        let mut binary = Vec::new();
        for value in [0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            push_f32(&mut binary, value);
        }
        for _ in 0..12 {
            binary.extend_from_slice(&0_u16.to_le_bytes());
        }
        for _ in 0..3 {
            for value in weight {
                push_f32(&mut binary, value);
            }
        }
        for value in times {
            push_f32(&mut binary, value);
        }
        for value in [0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0] {
            push_f32(&mut binary, value);
        }
        let inverse_offset = binary.len();
        for _ in 0..inverse_bind_count.unwrap_or(0) {
            for value in [
                1.0_f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ] {
                push_f32(&mut binary, value);
            }
        }
        fs::write(root.join("actor.bin"), &binary).unwrap();

        let mut nodes = vec![json!({"name":"Mesh","mesh":0,"skin":0})];
        for joint_index in 0..joint_count {
            let node_index = joint_index + 1;
            let mut node = json!({"name":format!("Joint{joint_index}")});
            if joint_index + 1 < joint_count {
                node["children"] = json!([node_index + 1]);
            }
            nodes.push(node);
        }
        if cycle && joint_count >= 2 {
            nodes[1]["children"] = json!([2]);
            nodes[2]["children"] = json!([1]);
        }
        let mut skin = json!({
            "name":"TestSkin",
            "joints":(1..=joint_count).collect::<Vec<_>>(),
            "skeleton":1
        });
        let mut buffer_views = vec![
            json!({"buffer":0,"byteOffset":0,"byteLength":36}),
            json!({"buffer":0,"byteOffset":36,"byteLength":24}),
            json!({"buffer":0,"byteOffset":60,"byteLength":48}),
            json!({"buffer":0,"byteOffset":108,"byteLength":8}),
            json!({"buffer":0,"byteOffset":116,"byteLength":24}),
        ];
        let mut accessors = vec![
            json!({"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}),
            json!({"bufferView":1,"componentType":5123,"count":3,"type":"VEC4"}),
            json!({"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"}),
            json!({"bufferView":3,"componentType":5126,"count":2,"type":"SCALAR"}),
            json!({"bufferView":4,"componentType":5126,"count":2,"type":"VEC3"}),
        ];
        if let Some(count) = inverse_bind_count {
            buffer_views
                .push(json!({"buffer":0,"byteOffset":inverse_offset,"byteLength":count * 64}));
            accessors
                .push(json!({"bufferView":5,"componentType":5126,"count":count,"type":"MAT4"}));
            skin["inverseBindMatrices"] = json!(5);
        }

        let document = json!({
            "asset":{"version":"2.0"},
            "scene":0,
            "scenes":[{"nodes":[0,1]}],
            "nodes":nodes,
            "meshes":[{"name":"Triangle","primitives":[{"attributes":{
                "POSITION":0,"JOINTS_0":1,"WEIGHTS_0":2
            }}]}],
            "skins":[skin],
            "animations":[{"name":"MoveRoot","samplers":[{"input":3,"output":4,"interpolation":"LINEAR"}],"channels":[{"sampler":0,"target":{"node":1,"path":"translation"}}]}],
            "buffers":[{"uri":"actor.bin","byteLength":binary.len()}],
            "bufferViews":buffer_views,
            "accessors":accessors
        });
        fs::write(&path, serde_json::to_vec_pretty(&document).unwrap()).unwrap();
        Self { root, path }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn push_f32(bytes: &mut Vec<u8>, value: f32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn source_id(path: &Path) -> AssetId {
    AssetId::from_legacy_external_path(AssetKind::GltfDocument, path).unwrap()
}

#[test]
fn owned_document_builds_skeleton_and_typed_clip_with_stable_bind_pose() {
    let fixture = Fixture::new(2, [2.0, 2.0, 0.0, 0.0], false, [0.0, 1.0], None);
    let document = GltfDocumentAsset::load(&fixture.path).unwrap();

    assert_eq!(document.buffers.len(), 1);
    assert_eq!(document.nodes[2].parent, Some(1));
    assert_eq!(document.meshes[0].primitives[0].positions.len(), 3);
    assert_eq!(
        document.meshes[0].primitives[0].weights.as_ref().unwrap()[0],
        [0.5, 0.5, 0.0, 0.0]
    );
    let vertices = document.meshes[0].primitives[0].skinned_vertices().unwrap();
    assert_eq!(vertices.len(), 3);
    assert_eq!(vertices[0].joints, [0; 4]);
    assert_eq!(vertices[0].weights, [0.5, 0.5, 0.0, 0.0]);
    assert_eq!(document.skins[0].inverse_bind.as_ref(), [Mat4::IDENTITY; 2]);

    let source = source_id(&fixture.path);
    let skeleton = SkeletonAsset::from_document(source.clone(), &document, 0).unwrap();
    assert_eq!(skeleton.joints.len(), 2);
    assert_eq!(skeleton.root, Some(0));
    assert_eq!(skeleton.joints[0].name.as_deref(), Some("Joint0"));
    assert_eq!(skeleton.joints[1].parent, Some(0));
    assert_eq!(skeleton.joints[0].translation, [0.0; 3]);

    let clip = AnimationClipAsset::from_document(source, &document, 0, &skeleton).unwrap();
    assert_eq!(clip.duration, 1.0);
    assert_eq!(clip.channels[0].target_joint, 0);
    assert_eq!(
        clip.channels[0].values,
        AnimationValues::Translation(Arc::from([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]]))
    );
    assert_eq!(GltfDocumentAsset::KIND, AssetKind::GltfDocument);
    assert_eq!(SkeletonAsset::KIND, AssetKind::Skeleton);
    assert_eq!(AnimationClipAsset::KIND, AssetKind::AnimationClip);

    let loaded_clip = AnimationClipAsset::load(&fixture.path).unwrap();
    assert_eq!(loaded_clip.channels.len(), 1);
    assert_eq!(loaded_clip.channels[0].target_joint, 0);
}

#[test]
fn node_cycle_is_reported_with_a_stable_error_code() {
    let fixture = Fixture::new(2, [1.0, 0.0, 0.0, 0.0], true, [0.0, 1.0], None);
    let error = load_document(&fixture.path).unwrap_err();
    assert_eq!(error, GltfError::Cycle);
    assert_eq!(error.code(), "Cycle");
}

#[test]
fn invalid_weights_are_rejected_without_clamping_or_truncation() {
    let negative = Fixture::new(1, [1.1, -0.1, 0.0, 0.0], false, [0.0, 1.0], None);
    assert_eq!(
        load_document(&negative.path).unwrap_err(),
        GltfError::NegativeWeight
    );

    let non_finite = Fixture::new(1, [f32::NAN, 0.0, 0.0, 0.0], false, [0.0, 1.0], None);
    assert_eq!(
        load_document(&non_finite.path).unwrap_err(),
        GltfError::NonFiniteWeight
    );

    let empty = Fixture::new(1, [0.0; 4], false, [0.0, 1.0], None);
    assert_eq!(
        load_document(&empty.path).unwrap_err(),
        GltfError::WeightSumTooSmall
    );
}

#[test]
fn joint_count_128_is_supported_and_129_remains_available_for_static_fallback() {
    let supported = Fixture::new(128, [1.0, 0.0, 0.0, 0.0], false, [0.0, 1.0], None);
    let document = load_document(&supported.path).unwrap();
    assert_eq!(
        SkeletonAsset::from_document(source_id(&supported.path), &document, 0)
            .unwrap()
            .joints
            .len(),
        128
    );

    let over_limit = Fixture::new(129, [1.0, 0.0, 0.0, 0.0], false, [0.0, 1.0], None);
    let document = load_document(&over_limit.path).unwrap();
    assert_eq!(document.skins[0].joints.len(), 129);
    assert_eq!(document.meshes[0].primitives[0].positions.len(), 3);
    let fallback = document.meshes[0].primitives[0].static_mesh_fallback();
    assert_eq!(fallback.positions.len(), 3);
    assert_eq!(
        fallback.indices,
        document.meshes[0].primitives[0].indices.as_ref()
    );
    assert_eq!(
        SkeletonAsset::from_document(source_id(&over_limit.path), &document, 0)
            .unwrap_err()
            .code(),
        "UnsupportedJointCount"
    );
}

#[test]
fn inverse_bind_count_and_animation_time_order_are_validated() {
    let bad_inverse = Fixture::new(2, [1.0, 0.0, 0.0, 0.0], false, [0.0, 1.0], Some(1));
    assert_eq!(
        load_document(&bad_inverse.path).unwrap_err(),
        GltfError::InvalidInverseBind
    );

    let bad_times = Fixture::new(1, [1.0, 0.0, 0.0, 0.0], false, [1.0, 1.0], None);
    assert_eq!(
        load_document(&bad_times.path).unwrap_err(),
        GltfError::NonMonotonicTime
    );
}

#[test]
fn animation_time_and_value_arity_must_match() {
    let fixture = Fixture::new(1, [1.0, 0.0, 0.0, 0.0], false, [0.0, 1.0], None);
    let mut document: Value = serde_json::from_slice(&fs::read(&fixture.path).unwrap()).unwrap();
    document["accessors"][4]["count"] = json!(1);
    fs::write(&fixture.path, serde_json::to_vec(&document).unwrap()).unwrap();
    assert_eq!(
        load_document(&fixture.path).unwrap_err(),
        GltfError::ChannelArityMismatch
    );
}
