use crate::renderer::pass::Pass;
use crate::renderer::resource::ResourceTag;
use std::any::TypeId;
use std::collections::HashMap;

pub(super) fn serialize(passes: &[Box<dyn Pass>]) -> String {
    let signatures: Vec<_> = passes.iter().map(|pass| pass.signature()).collect();
    let mut writers: HashMap<(TypeId, &'static str), String> = HashMap::new();
    let mut resource_edges = Vec::new();
    for (pass, signature) in passes.iter().zip(&signatures) {
        for slot in &signature.reads {
            if let Some(writer) = writers.get(&(slot.type_id, slot.name)) {
                resource_edges.push(serde_json::json!({
                    "from": writer,
                    "to": pass.name(),
                    "resource": slot.name,
                    "kind": "resource-read",
                }));
            }
        }
        for slot in &signature.writes {
            if let Some(writer) = writers.insert((slot.type_id, slot.name), pass.name().to_string())
            {
                resource_edges.push(serde_json::json!({
                    "from": writer,
                    "to": pass.name(),
                    "resource": slot.name,
                    "kind": "ordered-writes",
                }));
            }
        }
    }

    let pass_order: Vec<_> = passes.iter().map(|pass| pass.name().to_string()).collect();
    let execution_order_edges: Vec<_> = pass_order
        .windows(2)
        .map(|pair| serde_json::json!({ "from": pair[0], "to": pair[1] }))
        .collect();
    let transparent_enabled = signatures.iter().any(|signature| {
        signature.name == "Composite"
            && signature
                .reads
                .iter()
                .any(|slot| slot.name == crate::renderer::resource::TransparentColor::NAME)
    });
    let composite_overlay_order = if transparent_enabled {
        &crate::renderer::passes::composite::COMPOSITE_OVERLAY_ORDER[..]
    } else {
        &crate::renderer::passes::composite::COMPOSITE_OVERLAY_ORDER[1..]
    };
    let pass_nodes: Vec<_> = passes
        .iter()
        .zip(signatures)
        .enumerate()
        .map(|(index, (pass, signature))| {
            serde_json::json!({
                "index": index,
                "name": pass.name(),
                "reads": signature.reads.iter().map(|slot| slot.name).collect::<Vec<_>>(),
                "writes": signature.writes.iter().map(|slot| slot.name).collect::<Vec<_>>(),
            })
        })
        .collect();
    let artifact = serde_json::json!({
        "schema_version": 1,
        "execution_order": pass_order,
        "execution_order_edges": execution_order_edges,
        "resource_edges": resource_edges,
        "passes": pass_nodes,
        "composite_overlay_order": composite_overlay_order,
    });
    serde_json::to_string_pretty(&artifact).expect("render graph artifact is serializable")
}
