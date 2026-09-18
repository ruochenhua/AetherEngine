use serde_json::{json, Value};

pub fn case(id: &str) -> Value {
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

pub fn variant(id: &str) -> Value {
    json!({
        "id": id,
        "launcher_args_append": [],
        "time_override": null,
        "camera_override": null,
        "benchmark_override": null,
        "expected_result": {
            "kind": "Render", "diagnostics": [], "fallbacks": [], "metrics": [],
            "hashes": [], "probes": [], "graph": null
        }
    })
}
