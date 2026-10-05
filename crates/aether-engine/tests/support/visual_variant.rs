use serde_json::{json, Value};

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
