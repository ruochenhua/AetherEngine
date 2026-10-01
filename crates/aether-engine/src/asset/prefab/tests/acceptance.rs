use super::{document, instance, TestAssets};
use crate::asset::prefab::{
    instantiate, ComponentPatch, PrefabAsset, PrefabComponentKind, PrefabContext, PrefabError,
    PrefabOverrides, RemovedComponent,
};
use crate::asset::Asset;
use crate::ecs::components::{Name, PrefabNodeInstance, Visibility};
use crate::ecs::World;
use crate::editor::ComponentRecord;
use crate::scene::MaterialConfig;
use crate::visual_case::{ExpectedResult, ProbeValue, VisualManifest};
use serde_json::json;
use std::fs;
use std::path::Path;

#[test]
fn t8_prefab_roundtrip_acceptance() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap();
    let source =
        fs::read_to_string(workspace.join("tests/cases/t8_prefab_roundtrip.json")).unwrap();
    let manifest = VisualManifest::from_json_in(&source, workspace).unwrap();
    let materialized = manifest.materialize().unwrap();
    assert_eq!(materialized.len(), 1);
    assert_eq!(materialized[0].id, "t8_prefab_roundtrip__default");
    let expected_names = match materialized[0].expected_result.as_ref().unwrap() {
        ExpectedResult::Render { probes, .. } => {
            assert!(probes
                .iter()
                .all(|probe| probe.value == ProbeValue::Bool(true)));
            probes
                .iter()
                .map(|probe| probe.name.as_str())
                .collect::<Vec<_>>()
        }
        ExpectedResult::ExpectedError { .. } => panic!("T8.3 primary case must render"),
    };
    assert_eq!(
        expected_names,
        [
            "round_trip",
            "instance_id_remap",
            "typed_patch_validation",
            "transaction_rollback"
        ]
    );

    let original = document();
    let temp_root = std::env::temp_dir().join(format!(
        "aether-prefab-acceptance-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&temp_root).unwrap();
    let asset_path = temp_root.join("roundtrip.ron");
    original.save(&asset_path).unwrap();
    let loaded = PrefabAsset::load(&asset_path).unwrap();
    let round_trip = loaded.document == original;
    let _ = fs::remove_dir_all(temp_root);
    assert!(round_trip);

    let mut instance_config = instance(700);
    instance_config.overrides.patches = vec![
        ComponentPatch::TransformTranslation {
            instance_id: 11,
            value: [2.0, 1.0, 0.0],
        },
        ComponentPatch::Visibility {
            instance_id: 11,
            visible: true,
        },
    ];
    instance_config
        .overrides
        .removed_components
        .push(RemovedComponent {
            instance_id: 11,
            component: PrefabComponentKind::Name,
        });
    let mut effective = original.clone();
    instance_config.overrides.apply_to(&mut effective).unwrap();
    let invalid_patch = PrefabOverrides {
        patches: vec![ComponentPatch::MaterialConfig {
            instance_id: 11,
            config: MaterialConfig::default(),
        }],
        removed_components: vec![],
    };
    let typed_patch_validation = matches!(
        invalid_patch.apply_to(&mut original.clone()),
        Err(PrefabError::InvalidPatch { .. })
    ) && matches!(
        &effective.root.children[0].components[0],
        ComponentRecord::Transform {
            translation: [2.0, 1.0, 0.0],
            ..
        }
    ) && effective.root.children[0]
        .components
        .iter()
        .any(|component| matches!(component, ComponentRecord::Visibility { visible: true }));
    assert!(typed_patch_validation);

    let mut world = World::new();
    let mut assets = TestAssets::default();
    let mut first_context = PrefabContext::new(&instance_config, &mut assets);
    let first = instantiate(&original, &mut first_context, &mut world).unwrap();
    let second_instance = instance(701);
    let mut second_context = PrefabContext::new(&second_instance, &mut assets);
    let second = instantiate(&original, &mut second_context, &mut world).unwrap();
    let instance_id_remap = first.keys().copied().collect::<Vec<_>>() == [10, 11]
        && first.keys().eq(second.keys())
        && first
            .values()
            .zip(second.values())
            .all(|(left, right)| left != right)
        && world
            .query_one::<&PrefabNodeInstance>(first[&11])
            .get()
            .is_ok_and(|marker| marker.parent_instance_id == Some(10))
        && world
            .query_one::<&Visibility>(first[&11])
            .get()
            .is_ok_and(|visibility| visibility.0)
        && world.query_one::<&Name>(first[&11]).get().is_err();
    assert!(instance_id_remap);

    let mut rollback_world = World::new();
    let existing = rollback_world.spawn((Name("existing".into()),));
    let rollback_config = instance(702);
    let mut rollback_assets = TestAssets::default();
    let mut rollback_context = PrefabContext::new(&rollback_config, &mut rollback_assets);
    let transaction_rollback = super::super::instantiate::instantiate_with_insert_failure_after(
        &original,
        &mut rollback_context,
        &mut rollback_world,
        1,
    )
    .is_err()
        && rollback_world.len() == 1
        && rollback_world.query_one::<&Name>(existing).get().is_ok()
        && rollback_world
            .query::<&PrefabNodeInstance>()
            .iter()
            .next()
            .is_none();
    assert!(transaction_rollback);

    let Some(path) = std::env::var_os("AETHER_T8_PREFAB_EVENTS") else {
        return;
    };
    fs::write(
        Path::new(&path),
        serde_json::to_vec_pretty(&json!({
            "case_id": "t8_prefab_roundtrip",
            "kind": "Fixture",
            "events": [
                "prefab_saved_and_loaded_as_versioned_ron",
                "instance_overrides_applied_before_spawn",
                "stable_node_ids_mapped_to_fresh_runtime_entities",
                "component_tombstone_removed_instance_name",
                "partial_spawn_failure_rolled_back_created_entities"
            ],
            "probes": [
                { "name": "round_trip", "value": { "kind": "Bool", "value": round_trip } },
                { "name": "instance_id_remap", "value": { "kind": "Bool", "value": instance_id_remap } },
                { "name": "typed_patch_validation", "value": { "kind": "Bool", "value": typed_patch_validation } },
                { "name": "transaction_rollback", "value": { "kind": "Bool", "value": transaction_rollback } }
            ]
        }))
        .unwrap(),
    )
    .unwrap();
}
