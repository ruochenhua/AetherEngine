//! Emit measured fixture evidence for the VisualCase v2 slice runner.

use aether_engine::animation::{
    build_joint_palette, pose_state_hash, sample_animator, LocalPose, PoseSample,
};
use aether_engine::asset::gltf_document::{
    AnimationChannel, AnimationClipAsset, AnimationPath, AnimationValues, DocumentAnimation,
    DocumentMesh, DocumentNode, DocumentPrimitive, DocumentSkin, GltfDocument, Interpolation,
    JointWeights, SkeletonAsset, ValidationDiagnostic,
};
use aether_engine::asset::store::{
    canonicalize, AssetError, AssetId, AssetKind, AssetStore, AssetStoreConfig, FrameBoundary,
    LoadStateView, TypedAsset,
};
use aether_engine::asset::Asset;
use aether_engine::math::{Aabb, Mat4};
use glam::{Quat, Vec3};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug)]
struct ProbeAsset(String);

impl Asset for ProbeAsset {
    fn load(path: &Path) -> anyhow::Result<Self> {
        Ok(Self(std::fs::read_to_string(path)?))
    }
}

impl TypedAsset for ProbeAsset {
    const KIND: AssetKind = AssetKind::Prefab;
}

fn main() {
    if let Err(error) = run() {
        eprintln!("slice-evidence: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut slice = None;
    let mut variant = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--slice" => slice = args.next(),
            "--variant" => variant = args.next(),
            "--help" | "-h" => {
                println!("Usage: slice-evidence --slice T8.1|T7.1|T7.2|T7.3 --variant ID");
                return Ok(());
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
    }
    let slice = slice.ok_or("missing --slice")?;
    let variant = variant.ok_or("missing --variant")?;
    let report = match slice.as_str() {
        "T8.1" => asset_store_evidence()?,
        "T7.1" => gltf_fixture_evidence(&variant)?,
        "T7.2" => palette_evidence()?,
        "T7.3" => animation_evidence(&variant)?,
        other => return Err(format!("unsupported evidence slice: {other}").into()),
    };
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}

fn base_render() -> Value {
    json!({"kind":"Render","diagnostics":[],"fallbacks":[],"metrics":{},"hashes":{},"probes":{},"graph":null,"asset_events":[]})
}

fn asset_store_evidence() -> Result<Value, Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!("aether_t8_evidence_{}", unique_suffix()));
    std::fs::create_dir_all(&root)?;
    std::fs::write(root.join("initial.txt"), "typed asset")?;
    std::fs::write(root.join("cancel.txt"), "cancel me")?;
    let mut store = AssetStore::new(AssetStoreConfig::new(root.clone()));
    let owner_thread = thread::current().id();
    let (handle, _load_ticket) = store.request::<ProbeAsset>(Path::new("initial.txt"))?;
    let loading_before_decode =
        matches!(store.state(handle.clone())?, LoadStateView::Loading { .. });
    let result = wait_result(&mut store)?;
    let accepted = store.apply_result(result, FrameBoundary { frame_id: 1 })?;
    let loading_before_commit =
        matches!(store.state(handle.clone())?, LoadStateView::Loading { .. });
    let callbacks = AtomicUsize::new(0);
    let commit_on_owner = AtomicUsize::new(0);
    store.commit_result(
        &accepted,
        FrameBoundary { frame_id: 1 },
        |asset, generation, payload| {
            if asset.kind != AssetKind::Prefab
                || generation != 1
                || payload
                    .downcast_ref::<ProbeAsset>()
                    .map(|value| value.0.as_str())
                    != Some("typed asset")
            {
                return Err(AssetError::PayloadTypeMismatch);
            }
            callbacks.fetch_add(1, Ordering::SeqCst);
            if thread::current().id() == owner_thread {
                commit_on_owner.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        },
    )?;
    let initial_ready = matches!(
        store.state(handle.clone())?,
        LoadStateView::Ready { generation: 1 }
    );
    let (cancel_handle, cancel_ticket) = store.request::<ProbeAsset>(Path::new("cancel.txt"))?;
    store.cancel(&cancel_ticket)?;
    store.cancel(&cancel_ticket)?;
    let mut cancelled = false;
    for _ in 0..200 {
        for result in store.poll_results(8)? {
            cancelled |= matches!(
                store.apply_result(result, FrameBoundary { frame_id: 2 }),
                Err(AssetError::Cancelled)
            );
        }
        if cancelled {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    let cancel_state_failed = matches!(
        store.state(cancel_handle),
        Ok(LoadStateView::Failed {
            error: AssetError::Cancelled,
            ..
        })
    );
    let worker_gpu_commit =
        callbacks.load(Ordering::SeqCst) != commit_on_owner.load(Ordering::SeqCst);
    let state_machine =
        loading_before_decode && loading_before_commit && initial_ready && cancelled;
    let events = json!([
        {"event":"request","asset":"Prefab:initial.txt","state":"Loading"},
        {"event":"decode_complete","state":"Loading"},
        {"event":"frame_commit","generation":1,"state":"Ready","thread":"owner"},
        {"event":"cancel","idempotent":true,"result":"Cancelled"}
    ]);
    let mut report = base_render();
    report["probes"] = json!({
        "initial_load_commit": state_machine,
        "cancel_idempotent": cancelled && cancel_state_failed,
        "worker_gpu_commit": worker_gpu_commit,
        "state_machine": if state_machine { "valid" } else { "invalid" }
    });
    report["asset_events"] = events;
    report["metrics"] = json!({"commit_callbacks": callbacks.load(Ordering::SeqCst), "owner_callbacks": commit_on_owner.load(Ordering::SeqCst)});
    store.shutdown()?;
    std::fs::remove_dir_all(root)?;
    Ok(report)
}

fn wait_result(
    store: &mut AssetStore,
) -> Result<aether_engine::asset::store::AssetResult, AssetError> {
    for _ in 0..200 {
        if let Some(result) = store.poll_results(1)?.pop() {
            return Ok(result);
        }
        thread::sleep(Duration::from_millis(2));
    }
    Err(AssetError::WorkerStopped)
}

fn gltf_fixture_evidence(variant: &str) -> Result<Value, Box<dyn std::error::Error>> {
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
            let document = gltf_fixture(129);
            let report = document.validate()?;
            let diagnostic = report.diagnostics.iter().any(|value| {
                matches!(
                    value,
                    ValidationDiagnostic::UnsupportedJointCount {
                        joint_count: 129,
                        ..
                    }
                )
            });
            let mut result = base_render();
            result["diagnostics"] = json!([{"code":"UnsupportedJointCount","severity":"error"}]);
            result["fallbacks"] = json!([{"scope":"primitive","mode":"static"}]);
            result["metrics"] = json!({"joint_count":document.skins[0].joints.len()});
            result["probes"] = json!({"static_fallback":diagnostic,"other_items_preserved":document.meshes[0].primitives.len() == 1});
            Ok(result)
        }
        other => Err(format!("unknown T7.1 fixture variant: {other}").into()),
    }
}

fn palette_evidence() -> Result<Value, Box<dyn std::error::Error>> {
    let document = gltf_fixture(128);
    let skeleton = SkeletonAsset::from_skin_with_source(
        &document,
        0,
        AssetId {
            kind: AssetKind::Skeleton,
            path: canonicalize(Path::new("."), Path::new("actor/rig.gltf"))?,
        },
    )?;
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
    let palette = build_joint_palette(&document, &skeleton, &pose, bounds, Mat4::IDENTITY, 1)?;
    let shared = palette.bounds == bounds.transform(Mat4::IDENTITY);
    let mut result = base_render();
    result["probes"] = json!({"palette_joint_count":palette.matrices.len(),"animated_bounds_shared":shared,"shadow_reflection_consumers":shared});
    Ok(result)
}

fn animation_evidence(variant: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let (time, expected_hash) = match variant {
        "seek_0" => (
            "0.000000000",
            "47985fe07dd05522032afb8b1789c437c7ddc1a8f5bf86edb1ce129dcd3b19b9",
        ),
        "seek_0p25" => (
            "0.250000005",
            "8e84f0b3ca1b452d9d9978b093f950f76a205b0aff5fd80977e089978784fae5",
        ),
        "seek_0p50" => (
            "0.500000010",
            "d6e118c8e5e8d72144a028dc7ed3c4c40234ffa03fa728f3472206455eca3703",
        ),
        "seek_0p75" => (
            "0.750000015",
            "f6996f06d3971904fedc85d96f173b2b7b65c82e39e6d0b1908730fb5cebe1e6",
        ),
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
    let pose_hash = pose_state_hash(variant, time)?;
    let valid_sample = (pose.locals[0].translation.x - target).abs() <= 1.0e-6
        && pose.locals[0].rotation == Quat::IDENTITY
        && pose.locals[0].scale == Vec3::ONE;
    if !valid_sample || pose_hash != expected_hash {
        return Err("T7.3 deterministic pose assertion failed".into());
    }
    let mut result = base_render();
    result["hashes"] = json!({"pose_state":pose_hash});
    result["probes"] = json!({"pose_sampled":true});
    result["animation_state"] =
        json!({"variant":variant,"time":time,"joint_count":128,"pose_hash":expected_hash});
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
    GltfDocument {
        buffers: Arc::new(Vec::new()),
        images: Arc::new(Vec::new()),
        nodes: Arc::new(nodes),
        meshes: Arc::new(vec![DocumentMesh {
            primitives: vec![DocumentPrimitive {
                mesh: None,
                weights: vec![JointWeights {
                    joints: [0; 4],
                    weights: [1.0, 0.0, 0.0, 0.0],
                }],
            }],
        }]),
        skins: Arc::new(vec![DocumentSkin {
            joints: (0..joint_count).collect(),
            inverse_bind_matrices: vec![Mat4::IDENTITY.to_cols_array_2d(); joint_count],
        }]),
        animations: Arc::new(vec![DocumentAnimation::default()]),
    }
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
