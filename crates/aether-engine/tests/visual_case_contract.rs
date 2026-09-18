#[path = "support/visual_case.rs"]
mod fixtures;

use aether_engine::visual_case::VisualManifest;
use fixtures::{case, variant};
use serde_json::{json, Value};

fn parse(value: Value) -> Result<VisualManifest, aether_engine::visual_case::VisualCaseError> {
    VisualManifest::from_json(&json!([value]).to_string())
}

fn benchmark() -> Value {
    json!({"entity_count":1000,"seed":9001,"warmup":30,"samples":120,
        "feature_flags":{"visibility":false,"instancing":false},
        "baseline_commit":"0725ce0a36e25de4ea9f292208d44e4989cd4668",
        "device":{"os":"macOS 13.7.8","arch":"x86_64","driver":"Metal 3",
            "adapter":"Intel Iris Plus Graphics 650","resolution":[1280,720]},
        "paired":{"baseline_flags":{"instancing":false},"feature_flags":{"instancing":true},
            "equivalence":"rgba8_sha256 + visible_entity_sha256 + state_hash"}})
}

fn structured_case() -> Value {
    let mut value = case("structured");
    let mut render = variant("render");
    render["expected_result"] = json!({"kind":"Render",
        "diagnostics":[{"code":"LocalLightOverflow","severity":"warning"}],
        "fallbacks":[{"scope":"lighting","mode":"direction-only"}],
        "metrics":[{"name":"local_count","operator":"Eq","value":32.0}],
        "hashes":[{"name":"state_hash","sha256":"a".repeat(64)}],
        "probes":[
            {"name":"ready","value":{"kind":"Bool","value":true}},
            {"name":"count","value":{"kind":"U32","value":32}},
            {"name":"error","value":{"kind":"F32","value":{"value":0.0,"tolerance":0.015625}}},
            {"name":"pixel","value":{"kind":"Rgba8","value":[121,57,108,255]}},
            {"name":"status","value":{"kind":"Text","value":"pass"}},
            {"name":"state","value":{"kind":"Present"}}],
        "graph":{"reads":["SceneColor"],"writes":["Composite"],"edges":["SSR->VolumetricOverlay","SceneColor->SSR"]}});
    let mut error = variant("error");
    error["expected_result"] = json!({"kind":"ExpectedError","code":"Cycle",
        "diagnostics":[{"code":"Cycle","severity":"error"}]});
    render["benchmark_override"] = benchmark();
    value["variants"] = json!([render, error]);
    value
}

#[test]
fn frozen_structured_expectations_and_benchmark_blocks_are_accepted() {
    parse(structured_case()).unwrap();
    let mut value = case("benchmark");
    value["benchmark"] = benchmark();
    parse(value.clone()).unwrap();
    value["benchmark"].as_object_mut().unwrap().remove("paired");
    parse(value).unwrap(); // The frozen unpaired benchmark literals omit paired.
}

#[test]
fn materialized_ids_cannot_collide_with_other_cases_or_variants() {
    let mut base = case("base");
    base["variants"] = json!([variant("child")]);
    let input = json!([base, case("base__child")]).to_string();
    assert!(VisualManifest::from_json(&input)
        .unwrap()
        .materialize()
        .is_err());
    let mut left = case("a__b");
    left["variants"] = json!([variant("c")]);
    let mut right = case("a");
    right["variants"] = json!([variant("b__c")]);
    let input = json!([left, right]).to_string();
    assert!(VisualManifest::from_json(&input)
        .unwrap()
        .materialize()
        .is_err());
}

#[test]
fn materialization_preserves_expectations_overrides_and_independent_provenance() {
    let mut full = structured_case();
    full["time"]["fixed_dt"] = json!(0.0625);
    full["benchmark"] = benchmark();
    full["variants"][0]["benchmark_override"]["entity_count"] = json!(10000);
    full["variants"][0]["launcher_args_append"] = json!(["--debug-mode", "1"]);
    full["variants"][0]["time_override"] = full["time"].clone();
    full["variants"][0]["time_override"]["simulation_time"] = json!(1.0);
    full["variants"][0]["camera_override"] = json!({"source":"scene","override":{
        "position":[2.0,3.0,4.0],"rotation_xyzw":[0.0,0.0,0.0,1.0],"fov_deg":70.0,"near":0.25,"far":50.0}});
    full["reference"] = json!({"path":"tests/reference/base.png","sha256":"a".repeat(64),
        "source_commit":"b".repeat(40),"os":"macOS","driver":"Metal","adapter":"GPU",
        "runner_version":"2","width":1280,"height":720,"format":"png-rgba8"});
    let manifest = parse(full.clone()).unwrap();
    let source_before = serde_json::to_value(&manifest.cases()[0]).unwrap();
    let output = manifest.materialize().unwrap();
    let first = serde_json::to_value(&output[0]).unwrap();
    let second = serde_json::to_value(&output[1]).unwrap();
    assert_eq!(
        first["reference"]["path"],
        "tests/reference/structured__render.png"
    );
    assert_eq!(
        second["reference"]["path"],
        "tests/reference/structured__error.png"
    );
    for child in [&first, &second] {
        for (key, value) in child["reference"].as_object().unwrap() {
            if key != "path" {
                assert!(value.is_null(), "base provenance leaked: {key}");
            }
        }
        assert!(child["variants"].is_null());
        for field in ["scene", "render", "compare", "criteria"] {
            assert_eq!(child[field], source_before[field]);
        }
    }
    assert_eq!(first["time"], full["variants"][0]["time_override"]);
    assert_eq!(first["camera"], full["variants"][0]["camera_override"]);
    assert_eq!(
        first["benchmark"],
        full["variants"][0]["benchmark_override"]
    );
    assert_eq!(
        first["launcher_args"],
        json!(["--ssao", "--debug-mode", "1"])
    );
    assert_eq!(second["time"], full["time"]);
    assert_eq!(second["camera"], full["camera"]);
    assert_eq!(second["benchmark"], full["benchmark"]);
    assert_eq!(
        first["expected_result"],
        full["variants"][0]["expected_result"]
    );
    assert_eq!(
        second["expected_result"],
        full["variants"][1]["expected_result"]
    );
    assert_eq!(
        serde_json::to_value(&manifest.cases()[0]).unwrap(),
        source_before
    );
    let first_paths = output[0].artifacts("run-1").unwrap();
    let second_paths = output[1].artifacts("run-1").unwrap();
    assert_eq!(
        first_paths.output,
        "tests/reports/run-1/structured__render/output.png"
    );
    assert_eq!(
        first_paths.diff,
        "tests/reports/run-1/structured__render/diff.png"
    );
    assert_eq!(
        first_paths.report,
        "tests/reports/run-1/structured__render/report.html"
    );
    assert_ne!(first_paths, second_paths);
    assert!(output[0].artifacts("../escape").is_err());
}

#[test]
fn nested_objects_reject_unknown_and_missing_required_fields() {
    let mut full = structured_case();
    full["benchmark"] = benchmark();
    full["camera"]["override"] =
        json!({"position":[0,1,2],"rotation_xyzw":[0,0,0,1],"fov_deg":60,"near":0.1,"far":100});
    full["variants"][0]["time_override"] = full["time"].clone();
    full["variants"][0]["camera_override"] = full["camera"].clone();
    parse(full.clone()).unwrap();
    let paths = [
        "/render",
        "/time",
        "/camera",
        "/camera/override",
        "/reference",
        "/compare",
        "/benchmark",
        "/benchmark/device",
        "/benchmark/paired",
        "/variants/0",
        "/variants/0/time_override",
        "/variants/0/camera_override",
        "/variants/0/camera_override/override",
        "/variants/0/benchmark_override",
        "/variants/0/expected_result",
        "/variants/0/expected_result/diagnostics/0",
        "/variants/0/expected_result/fallbacks/0",
        "/variants/0/expected_result/metrics/0",
        "/variants/0/expected_result/hashes/0",
        "/variants/0/expected_result/probes/2",
        "/variants/0/expected_result/probes/2/value",
        "/variants/0/expected_result/probes/2/value/value",
        "/variants/0/expected_result/graph",
        "/variants/1/expected_result",
    ];
    for path in paths {
        let mut unknown = full.clone();
        unknown.pointer_mut(path).unwrap()["surprise"] = json!(true);
        assert!(parse(unknown).is_err(), "accepted unknown field at {path}");
        for field in full.pointer(path).unwrap().as_object().unwrap().keys() {
            if field == "paired" {
                continue;
            }
            let mut missing = full.clone();
            missing
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(parse(missing).is_err(), "accepted missing {path}/{field}");
        }
    }
}

#[test]
fn invalid_nested_values_and_camera_parameters_are_rejected() {
    let mut full = structured_case();
    full["benchmark"] = benchmark();
    full["camera"]["override"] =
        json!({"position":[0,1,2],"rotation_xyzw":[0,0,0,1],"fov_deg":60,"near":0.1,"far":100});
    parse(full.clone()).unwrap();
    for (path, value) in [
        ("/variants/0/expected_result/kind", json!("render")),
        ("/variants/1/expected_result/kind", json!("expected_error")),
        ("/variants/1/expected_result/code", json!("")),
        (
            "/variants/0/expected_result/diagnostics/0/severity",
            json!("bogus"),
        ),
        ("/variants/0/expected_result/diagnostics/0/code", json!("")),
        ("/variants/0/expected_result/fallbacks/0/scope", json!("")),
        (
            "/variants/0/expected_result/metrics/0/operator",
            json!("eq"),
        ),
        ("/variants/0/expected_result/metrics/0/value", Value::Null),
        ("/variants/0/expected_result/hashes/0/sha256", json!("bad")),
        (
            "/variants/0/expected_result/probes/2/value/value/tolerance",
            json!(-1),
        ),
        (
            "/variants/0/expected_result/probes/2/value/value/value",
            json!(1e100),
        ),
        (
            "/variants/0/expected_result/probes/3/value/value",
            json!([256, 0, 0, 0]),
        ),
        ("/variants/0/expected_result/graph/edges", json!([""])),
        ("/benchmark/entity_count", json!(0)),
        ("/benchmark/samples", json!(0)),
        ("/benchmark/baseline_commit", json!("v0.1.0")),
        ("/benchmark/device/resolution", json!([0, 720])),
        ("/benchmark/device/driver", json!("")),
        ("/benchmark/paired/equivalence", json!("")),
        ("/camera/source", json!("override")),
        ("/camera/override/fov_deg", json!(0)),
        ("/camera/override/fov_deg", json!(180)),
        ("/camera/override/near", json!(0)),
        ("/camera/override/far", json!(0.1)),
        ("/camera/override/rotation_xyzw", json!([0, 0, 0, 0])),
        ("/camera/override/position", json!([1e100, 0, 0])),
        ("/time/max_seek_steps", json!(4097)),
    ] {
        let mut invalid = full.clone();
        *invalid.pointer_mut(path).unwrap() = value;
        assert!(parse(invalid).is_err(), "accepted invalid {path}");
    }
}
