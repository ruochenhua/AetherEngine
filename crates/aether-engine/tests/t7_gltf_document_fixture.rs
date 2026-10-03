#[path = "support/t7_gltf.rs"]
mod fixtures;

use aether_engine::asset::loaders::gltf::{load_document, GltfError, SkeletonAsset};
use aether_engine::asset::{AssetId, AssetKind};
use aether_engine::visual_case::{ExpectedResult, ProbeValue, VisualManifest};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}

fn validate_visual_case(root: &Path) {
    let case_path = root.join("tests/cases/t7_gltf_document_fixture.json");
    let source = fs::read_to_string(case_path).unwrap();
    let manifest = VisualManifest::from_json_in(&source, root).unwrap();
    let materialized = manifest.materialize().unwrap();
    assert_eq!(
        materialized
            .iter()
            .map(|variant| variant.id.as_str())
            .collect::<Vec<_>>(),
        [
            "t7_gltf_document_fixture__valid_128",
            "t7_gltf_document_fixture__malformed_graph",
            "t7_gltf_document_fixture__invalid_129"
        ]
    );
    match materialized[0].expected_result.as_ref().unwrap() {
        ExpectedResult::Render {
            metrics, probes, ..
        } => {
            assert_eq!(metrics[0].name, "joint_count");
            assert_eq!(
                probes
                    .iter()
                    .map(|probe| probe.name.as_str())
                    .collect::<Vec<_>>(),
                ["weight_validation", "graph_acyclic"]
            );
            assert_eq!(probes[0].value, ProbeValue::Bool(true));
        }
        ExpectedResult::ExpectedError { .. } => panic!("valid_128 must be a render result"),
    }
    assert!(matches!(
        materialized[1].expected_result.as_ref().unwrap(),
        ExpectedResult::ExpectedError { code, .. } if code == "Cycle"
    ));
    match materialized[2].expected_result.as_ref().unwrap() {
        ExpectedResult::Render {
            diagnostics,
            fallbacks,
            probes,
            ..
        } => {
            assert_eq!(diagnostics[0].code, "UnsupportedJointCount");
            assert_eq!(fallbacks[0].scope, "primitive");
            assert_eq!(fallbacks[0].mode, "static");
            assert_eq!(
                probes
                    .iter()
                    .map(|probe| probe.name.as_str())
                    .collect::<Vec<_>>(),
                ["static_fallback", "other_items_preserved"]
            );
        }
        ExpectedResult::ExpectedError { .. } => panic!("129 joints use a per-primitive fallback"),
    }
}

fn write_events(events: serde_json::Value) {
    let Some(path) = std::env::var_os("AETHER_T7_GLTF_EVENTS") else {
        return;
    };
    fs::write(path, serde_json::to_vec_pretty(&events).unwrap()).unwrap();
}

#[test]
fn t7_gltf_document_fixture() {
    let root = workspace_root();
    validate_visual_case(&root);

    let valid = fixtures::Fixture::new(128, false, false);
    let valid_document = load_document(&valid.path).unwrap();
    let source = AssetId::from_path(
        AssetKind::GltfDocument,
        &valid.root,
        Path::new("actor.gltf"),
    )
    .unwrap();
    let valid_skeleton = SkeletonAsset::from_document(source, &valid_document, 0).unwrap();
    let valid_primitive = &valid_document.meshes[0].primitives[0];
    let weight_validation = valid_primitive.weights.as_ref().is_some_and(|weights| {
        weights
            .iter()
            .all(|weights| (weights.iter().sum::<f32>() - 1.0).abs() <= 1.0e-4)
    });
    let graph_acyclic = valid_document.nodes.len() == 129
        && valid_skeleton.joints.len() == 128
        && valid_skeleton.joints[1].parent == Some(0);

    let malformed = fixtures::Fixture::new(2, true, false);
    let malformed_error = load_document(&malformed.path).unwrap_err();
    assert_eq!(malformed_error, GltfError::Cycle);

    let over_limit = fixtures::Fixture::new(129, false, true);
    let over_limit_document = load_document(&over_limit.path).unwrap();
    let over_limit_source = AssetId::from_path(
        AssetKind::GltfDocument,
        &over_limit.root,
        Path::new("actor.gltf"),
    )
    .unwrap();
    let unsupported =
        SkeletonAsset::from_document(over_limit_source, &over_limit_document, 0).unwrap_err();
    assert_eq!(unsupported.code(), "UnsupportedJointCount");
    let fallback = over_limit_document.meshes[0].primitives[0].static_mesh_fallback();
    let static_fallback = fallback.positions.len() == 3
        && fallback.indices.len() == over_limit_document.meshes[0].primitives[0].indices.len();
    let other_items_preserved = over_limit_document
        .nodes
        .iter()
        .any(|node| node.name.as_deref() == Some("OtherStatic") && node.mesh == Some(1))
        && over_limit_document.meshes.get(1).is_some();

    assert!(weight_validation);
    assert!(graph_acyclic);
    assert!(static_fallback);
    assert!(other_items_preserved);
    write_events(json!({
        "case_id":"t7_gltf_document_fixture",
        "kind":"Fixture",
        "variants":[
            {
                "id":"valid_128","kind":"Render","diagnostics":[],"fallbacks":[],
                "metrics":{"joint_count":valid_skeleton.joints.len()},
                "probes":[
                    {"name":"weight_validation","value":{"kind":"Bool","value":weight_validation}},
                    {"name":"graph_acyclic","value":{"kind":"Bool","value":graph_acyclic}}
                ]
            },
            {"id":"malformed_graph","kind":"ExpectedError","code":malformed_error.code()},
            {
                "id":"invalid_129","kind":"Render",
                "diagnostics":[{"code":unsupported.code(),"severity":"error"}],
                "fallbacks":[{"scope":"primitive","mode":"static"}],
                "metrics":{"joint_count":over_limit_document.skins[0].joints.len()},
                "probes":[
                    {"name":"static_fallback","value":{"kind":"Bool","value":static_fallback}},
                    {"name":"other_items_preserved","value":{"kind":"Bool","value":other_items_preserved}}
                ]
            }
        ]
    }));
}
