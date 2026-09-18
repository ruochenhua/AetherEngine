use aether_engine::visual_case::{ReferenceState, VisualManifest};
use serde_json::{json, Value};

fn case(id: &str) -> Value {
    json!({
        "manifest_version": 2,
        "id": id,
        "scene": "scenes/01_deferred.ron",
        "launcher_args": ["--ssao"],
        "render": {
            "width": 1280,
            "height": 720,
            "frames": 1,
            "no_gui_overlay": true,
            "png": "rgba8"
        },
        "time": {
            "mode": "seek",
            "simulation_time": 0.5,
            "fixed_dt": 0.1,
            "max_substeps": 4,
            "max_seek_steps": 4096
        },
        "camera": {"source": "scene", "override": null},
        "reference": {
            "path": "tests/reference/01_deferred.png",
            "sha256": null,
            "source_commit": null,
            "os": null,
            "driver": null,
            "adapter": null,
            "runner_version": null,
            "width": null,
            "height": null,
            "format": null
        },
        "compare": {
            "algorithm": "rgba8_normalized",
            "exact_hash": false,
            "allow_degraded_compare": false,
            "ssim_min": 0.99,
            "mae_max": 0.01,
            "diff_percent_max": 0.01
        },
        "criteria": ["the scene is visible"],
        "benchmark": null,
        "variants": null
    })
}

fn parse(cases: Vec<Value>) -> Result<VisualManifest, aether_engine::visual_case::VisualCaseError> {
    VisualManifest::from_json(&serde_json::to_string(&cases).unwrap())
}

#[test]
fn valid_v2_fixture_is_reference_pending() {
    let manifest = parse(vec![case("primary")]).unwrap();
    assert_eq!(
        manifest.cases()[0].reference_state(),
        ReferenceState::Pending
    );
    assert_eq!(manifest.materialize().unwrap()[0].id, "primary");
}

#[test]
fn complete_provenance_is_releasable() {
    let mut fixture = case("release");
    let reference = fixture["reference"].as_object_mut().unwrap();
    reference.insert("sha256".into(), json!("a".repeat(64)));
    reference.insert("source_commit".into(), json!("b".repeat(40)));
    reference.insert("os".into(), json!("macos-15"));
    reference.insert("driver".into(), json!("metal-1"));
    reference.insert("adapter".into(), json!("Apple GPU"));
    reference.insert("runner_version".into(), json!("2.0.0"));
    reference.insert("width".into(), json!(1280));
    reference.insert("height".into(), json!(720));
    reference.insert("format".into(), json!("png-rgba8"));

    let manifest = parse(vec![fixture]).unwrap();
    assert_eq!(
        manifest.cases()[0].reference_state(),
        ReferenceState::Releasable
    );
}

#[test]
fn unknown_missing_and_duplicate_ids_are_rejected() {
    let mut unknown = case("unknown");
    unknown["surprise"] = json!(true);
    assert!(parse(vec![unknown]).is_err());

    let mut missing = case("missing");
    missing.as_object_mut().unwrap().remove("camera");
    assert!(parse(vec![missing]).is_err());

    assert!(parse(vec![case("same"), case("same")]).is_err());
    assert!(parse(vec![case("")]).is_err());
}

#[test]
fn path_escapes_and_invalid_numeric_thresholds_are_rejected() {
    let mut scene_escape = case("scene_escape");
    scene_escape["scene"] = json!("scenes/../secrets.ron");
    assert!(parse(vec![scene_escape]).is_err());

    let mut reference_escape = case("reference_escape");
    reference_escape["reference"]["path"] = json!("/tmp/reference.png");
    assert!(parse(vec![reference_escape]).is_err());

    let mut threshold = case("threshold");
    threshold["compare"]["ssim_min"] = json!(0.98);
    assert!(parse(vec![threshold]).is_err());
}

#[test]
fn partial_or_malformed_provenance_is_rejected() {
    let mut partial = case("partial");
    partial["reference"]["sha256"] = json!("a".repeat(64));
    assert!(parse(vec![partial]).is_err());

    let mut malformed = case("malformed");
    let reference = malformed["reference"].as_object_mut().unwrap();
    reference.insert("sha256".into(), json!("NOT-A-HASH"));
    reference.insert("source_commit".into(), json!("b".repeat(40)));
    reference.insert("os".into(), json!("macos-15"));
    reference.insert("driver".into(), json!("metal-1"));
    reference.insert("adapter".into(), json!("Apple GPU"));
    reference.insert("runner_version".into(), json!("2.0.0"));
    reference.insert("width".into(), json!(1280));
    reference.insert("height".into(), json!(720));
    reference.insert("format".into(), json!("png-rgba8"));
    assert!(parse(vec![malformed]).is_err());
}

#[test]
fn variants_materialize_to_independent_validated_case_ids() {
    let mut fixture = case("base");
    fixture["variants"] = json!([
        {
            "id": "half",
            "launcher_args_append": ["--debug-mode", "1"],
            "time_override": {
                "mode": "seek", "simulation_time": 0.5, "fixed_dt": 0.1,
                "max_substeps": 4, "max_seek_steps": 4096
            },
            "camera_override": null,
            "expected_result": {
                "kind": "render", "diagnostics": [], "fallbacks": [], "metrics": {}
            }
        },
        {
            "id": "one",
            "launcher_args_append": [],
            "time_override": {
                "mode": "seek", "simulation_time": 1.0, "fixed_dt": 0.1,
                "max_substeps": 4, "max_seek_steps": 4096
            },
            "camera_override": null,
            "expected_result": {
                "kind": "expected_error", "code": "E_TEST", "diagnostics": ["expected"]
            }
        }
    ]);
    let manifest = parse(vec![fixture]).unwrap();
    let materialized = manifest.materialize().unwrap();
    assert_eq!(materialized[0].id, "base__half");
    assert_eq!(materialized[1].id, "base__one");
    assert_eq!(materialized[0].launcher_args.last().unwrap(), "1");

    let mut duplicate = case("base");
    duplicate["variants"] = json!([variant("same"), variant("same")]);
    assert!(parse(vec![duplicate]).is_err());

    let mut invalid = case("base");
    invalid["variants"] = json!([variant("bad/id")]);
    assert!(parse(vec![invalid]).is_err());
}

fn variant(id: &str) -> Value {
    json!({
        "id": id,
        "launcher_args_append": [],
        "time_override": null,
        "camera_override": null,
        "expected_result": {
            "kind": "render", "diagnostics": [], "fallbacks": [], "metrics": {}
        }
    })
}
