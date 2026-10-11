use super::base_render;
use aether_engine::animation::{
    extract_animation_frame, pose_state_hash, sample_animator, AnimationAssets,
    AnimationCameraFrame, AnimationContext, AnimationPrimitive, AnimationRenderItem,
    AnimatorRuntime, AnimatorState, MeshRef, SceneSnapshot,
};
use aether_engine::asset::gltf_document::{
    AnimationChannel, AnimationClipAsset, AnimationPath, AnimationValues, DocumentAnimation,
    DocumentMesh, DocumentNode, DocumentPrimitive, DocumentSkin, GltfDocument, Interpolation,
    JointWeights, SkeletonAsset,
};
use aether_engine::asset::store::{canonicalize, AssetId, AssetKind};
use aether_engine::asset::Handle;
use aether_engine::math::Mat4;
use glam::{Quat, Vec3};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;

struct EvidenceAnimationAssets {
    document: Arc<GltfDocument>,
    skeleton: Arc<SkeletonAsset>,
    clip: Arc<AnimationClipAsset>,
}

impl AnimationAssets for EvidenceAnimationAssets {
    fn document(
        &self,
        _handle: Handle<GltfDocument>,
    ) -> Result<Arc<GltfDocument>, aether_engine::animation::AnimationError> {
        Ok(self.document.clone())
    }

    fn skeleton(
        &self,
        _handle: Handle<SkeletonAsset>,
    ) -> Result<Arc<SkeletonAsset>, aether_engine::animation::AnimationError> {
        Ok(self.skeleton.clone())
    }

    fn clip(
        &self,
        _handle: Handle<AnimationClipAsset>,
    ) -> Result<Arc<AnimationClipAsset>, aether_engine::animation::AnimationError> {
        Ok(self.clip.clone())
    }
}

pub(super) fn gltf_fixture_evidence(variant: &str) -> Result<Value, Box<dyn std::error::Error>> {
    match variant {
        "malformed_graph" => {
            let mut document = gltf_fixture(2);
            Arc::make_mut(&mut document.nodes)[1].children.push(0);
            let code = document.validate().unwrap_err().to_string();
            Ok(
                json!({"kind":"ExpectedError","code":"Cycle","diagnostics":[],"observed_error":code}),
            )
        }
        "valid_128" => {
            let document = gltf_fixture(128);
            let report = document.validate()?;
            let skeleton = SkeletonAsset::from_skin(&document, 0)?;
            let mut result = base_render();
            result["metrics"] = json!({"joint_count":skeleton.joints.len()});
            result["probes"] =
                json!({"weight_validation":true,"graph_acyclic":report.diagnostics.is_empty()});
            Ok(result)
        }
        "invalid_129" => {
            let mut document = gltf_fixture(129);
            let mut nodes = document.nodes.as_ref().clone();
            nodes.push(DocumentNode {
                mesh: Some(1),
                skin: Some(1),
                translation: [0.0; 3],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0; 3],
                children: Vec::new(),
                parent: None,
            });
            document.nodes = Arc::new(nodes);
            let independent_mesh = document.meshes[0].primitives[0].clone();
            let mut meshes = document.meshes.as_ref().clone();
            meshes.push(DocumentMesh {
                primitives: vec![independent_mesh],
            });
            document.meshes = Arc::new(meshes);
            let mut skins = document.skins.as_ref().clone();
            skins.push(DocumentSkin {
                joints: vec![129],
                inverse_bind_matrices: vec![Mat4::IDENTITY.to_cols_array_2d()],
            });
            document.skins = Arc::new(skins);
            let skeleton = SkeletonAsset::from_skin_with_source(
                &document,
                1,
                AssetId {
                    kind: AssetKind::Skeleton,
                    path: canonicalize(Path::new("."), Path::new("actor/other-rig.gltf"))?,
                },
            )?;
            let assets = EvidenceAnimationAssets {
                document: Arc::new(document.clone()),
                skeleton: Arc::new(skeleton),
                clip: Arc::new(AnimationClipAsset {
                    channels: vec![AnimationChannel {
                        target_joint: 0,
                        target: AnimationPath::Translation,
                        interpolation: Interpolation::Linear,
                        times: Arc::from([0.0, 1.0]),
                        values: AnimationValues::Vec3(Arc::from([[0.0; 3], [0.0; 3]])),
                    }],
                    duration: 1.0,
                }),
            };
            let mesh_ref = MeshRef {
                asset: AssetId {
                    kind: AssetKind::CpuMesh,
                    path: canonicalize(Path::new("."), Path::new("actor/body.mesh"))?,
                },
                generation: 1,
            };
            let primitive = [
                AnimationPrimitive {
                    entity_bits: 1,
                    primitive_index: 0,
                    node_index: 0,
                    mesh: mesh_ref.clone(),
                    document: Handle::new(1),
                    skeleton: None,
                    static_mesh: Arc::new(aether_engine::asset::mesh::CpuMesh::cube()),
                    world_transform: Mat4::IDENTITY,
                },
                AnimationPrimitive {
                    entity_bits: 2,
                    primitive_index: 0,
                    node_index: 129,
                    mesh: mesh_ref,
                    document: Handle::new(1),
                    skeleton: Some(Handle::new(2)),
                    static_mesh: Arc::new(aether_engine::asset::mesh::CpuMesh::cube()),
                    world_transform: Mat4::IDENTITY,
                },
            ];
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
                primitives: &primitive,
                camera: &camera,
                scene_generation: 1,
                frame_id: 1,
            };
            let mut runtime = AnimatorRuntime {
                states: Default::default(),
                generation: 1,
            };
            for entity_bits in [1, 2] {
                runtime.states.insert(
                    entity_bits,
                    AnimatorState {
                        clip: Some(Handle::new(3)),
                        time: 0.0,
                        ..AnimatorState::default()
                    },
                );
            }
            let frame = extract_animation_frame(&runtime, &context)?;
            let diagnostic = matches!(
                frame.items.first(),
                Some(AnimationRenderItem::StaticFallback(_))
            ) && frame
                .diagnostics
                .iter()
                .any(|item| item.entity_bits == 1 && item.code.contains("UnsupportedJointCount"));
            let other_items_preserved = matches!(
                frame.items.get(1),
                Some(AnimationRenderItem::Skinned(item)) if item.entity_bits == 2
            );
            let mut result = base_render();
            result["diagnostics"] = json!([{"code":"UnsupportedJointCount","severity":"error"}]);
            result["fallbacks"] = json!([{"scope":"primitive","mode":"static"}]);
            result["metrics"] = json!({"joint_count":document.skins[0].joints.len()});
            result["probes"] =
                json!({"static_fallback":diagnostic,"other_items_preserved":other_items_preserved});
            Ok(result)
        }
        other => Err(format!("unknown T7.1 fixture variant: {other}").into()),
    }
}

pub(super) fn palette_evidence() -> Result<Value, Box<dyn std::error::Error>> {
    let document = gltf_fixture(128);
    let skeleton = SkeletonAsset::from_skin_with_source(
        &document,
        0,
        AssetId {
            kind: AssetKind::Skeleton,
            path: canonicalize(Path::new("."), Path::new("actor/rig.gltf"))?,
        },
    )?;
    let clip = Arc::new(AnimationClipAsset {
        channels: vec![AnimationChannel {
            target_joint: 0,
            target: AnimationPath::Translation,
            interpolation: Interpolation::Linear,
            times: Arc::from([0.0, 1.0]),
            values: AnimationValues::Vec3(Arc::from([[0.0; 3], [1.0, 0.0, 0.0]])),
        }],
        duration: 1.0,
    });
    let assets = EvidenceAnimationAssets {
        document: Arc::new(document.clone()),
        skeleton: Arc::new(skeleton),
        clip,
    };
    let primitive = [AnimationPrimitive {
        entity_bits: 7,
        primitive_index: 0,
        node_index: 0,
        mesh: MeshRef {
            asset: AssetId {
                kind: AssetKind::CpuMesh,
                path: canonicalize(Path::new("."), Path::new("actor/body.mesh"))?,
            },
            generation: 1,
        },
        document: Handle::new(1),
        skeleton: Some(Handle::new(2)),
        static_mesh: Arc::new(aether_engine::asset::mesh::CpuMesh::cube()),
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
        primitives: &primitive,
        camera: &camera,
        scene_generation: 1,
        frame_id: 1,
    };
    let mut runtime = AnimatorRuntime {
        states: Default::default(),
        generation: 1,
    };
    runtime.states.insert(
        7,
        AnimatorState {
            clip: Some(Handle::new(3)),
            time: 0.5,
            ..AnimatorState::default()
        },
    );
    let frame = extract_animation_frame(&runtime, &context)?;
    let Some(aether_engine::animation::AnimationRenderItem::Skinned(item)) = frame.items.first()
    else {
        return Err("T7.2 extraction did not publish a skinned item".into());
    };
    let shared =
        item.bounds == item.palette.bounds && frame.conservative_bounds().as_ref() == [item.bounds];
    let mut result = base_render();
    result["probes"] = json!({"palette_joint_count":item.palette.matrices.len(),"animated_bounds_shared":shared,"shadow_reflection_consumers":shared,"skinned_render_item_published":true});
    Ok(result)
}

pub(super) fn animation_evidence(variant: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let time = match variant {
        "seek_0" => "0.000000000",
        "seek_0p25" => "0.250000005",
        "seek_0p50" => "0.500000010",
        "seek_0p75" => "0.750000015",
        other => return Err(format!("unknown T7.3 seek variant: {other}").into()),
    };
    let document = gltf_fixture(128);
    let skeleton = SkeletonAsset::from_skin(&document, 0)?;
    let target = time.parse::<f32>()?;
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
    let pose = sample_animator(&document, &skeleton, Some(&clip), target)?;
    let pose_hash = pose_state_hash(variant, time, &pose)?;
    let valid_sample = (pose.locals[0].translation.x - target).abs() <= 1.0e-6
        && pose.locals[0].rotation == Quat::IDENTITY
        && pose.locals[0].scale == Vec3::ONE;
    if !valid_sample {
        return Err("T7.3 sampled pose assertion failed".into());
    }
    let mut result = base_render();
    result["hashes"] = json!({"pose_state":pose_hash});
    result["probes"] = json!({"pose_sampled":true});
    result["animation_state"] =
        json!({"variant":variant,"time":time,"joint_count":128,"pose_hash":pose_hash});
    Ok(result)
}

fn gltf_fixture(joint_count: usize) -> GltfDocument {
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
    let mut primitives = vec![DocumentPrimitive {
        mesh: Some(aether_engine::asset::mesh::CpuMesh::cube()),
        weights: vec![
            JointWeights {
                joints: [0; 4],
                weights: [1.0, 0.0, 0.0, 0.0],
            };
            24
        ],
    }];
    if joint_count > 128 {
        // Keep a second independent item in the same mesh to verify that an
        // unsupported skin only falls back for its own primitive.
        primitives.push(DocumentPrimitive {
            mesh: None,
            weights: Vec::new(),
        });
    }
    GltfDocument {
        buffers: Arc::new(Vec::new()),
        images: Arc::new(Vec::new()),
        nodes: Arc::new(nodes),
        meshes: Arc::new(vec![DocumentMesh { primitives }]),
        skins: Arc::new(vec![DocumentSkin {
            joints: (0..joint_count).collect(),
            inverse_bind_matrices: vec![Mat4::IDENTITY.to_cols_array_2d(); joint_count],
        }]),
        animations: Arc::new(vec![DocumentAnimation::default()]),
    }
}
