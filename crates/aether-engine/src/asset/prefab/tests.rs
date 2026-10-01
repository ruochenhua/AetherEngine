use super::*;
use crate::asset::Asset;
use crate::ecs::components::{Name, PrefabInstanceRoot, PrefabNodeInstance, Visibility};
use crate::ecs::World;
use crate::editor::{ComponentKind, ComponentRecord};
use crate::renderer::renderable::MaterialUniform;
use crate::scene::{LightConfig, MaterialConfig, MeshRef};
use std::fs;

fn transform(x: f32) -> ComponentRecord {
    ComponentRecord::Transform {
        translation: [x, 0.0, 0.0],
        rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0; 3],
    }
}

fn node(instance_id: u64, name: &str, children: Vec<PrefabNode>) -> PrefabNode {
    PrefabNode {
        instance_id,
        name: name.into(),
        components: vec![
            transform(0.0),
            ComponentRecord::Visibility { visible: false },
        ],
        children,
    }
}

fn document() -> PrefabDocument {
    PrefabDocument {
        schema_version: PREFAB_SCHEMA_VERSION,
        root: node(10, "Root", vec![node(11, "Child", vec![])]),
    }
}

fn instance(instance_id: u64) -> PrefabInstanceConfig {
    PrefabInstanceConfig {
        instance_id,
        prefab_asset: "assets/prefabs/test.ron".into(),
        overrides: PrefabOverrides::default(),
    }
}

#[derive(Default)]
struct TestAssets {
    fail_mesh: bool,
}

impl PrefabAssets for TestAssets {
    fn resolve_mesh(
        &mut self,
        source: &MeshRef,
    ) -> Result<crate::ecs::components::MeshHandle, PrefabError> {
        if self.fail_mesh {
            return Err(PrefabError::Dependency("test mesh error".into()));
        }
        Err(PrefabError::UnsupportedComponent(format!(
            "unexpected mesh {source:?}"
        )))
    }

    fn resolve_material(
        &mut self,
        _config: &MaterialConfig,
    ) -> Result<ResolvedPrefabMaterial, PrefabError> {
        Ok(ResolvedPrefabMaterial {
            uniform: MaterialUniform::default(),
            transparent: None,
        })
    }
}

#[test]
fn prefab_ron_roundtrip_preserves_hierarchy_and_component_values() {
    let original = document();

    let parsed = PrefabDocument::from_ron(&original.to_ron().unwrap()).unwrap();

    assert_eq!(parsed, original);
    assert_eq!(parsed.root.children[0].instance_id, 11);
}

#[test]
fn schema_v1_transform_migrates_into_the_closed_component_records() {
    let source = r#"(
        schema_version: 1,
        root: (
            instance_id: 7,
            name: "Legacy",
            transform: Some((translation: (3.0, 2.0, 1.0), rotation: (0.0, 0.0, 0.0, 1.0), scale: (1.0, 1.0, 1.0))),
        ),
    )"#;

    let migrated = PrefabDocument::from_ron(source).unwrap();

    assert_eq!(migrated.schema_version, PREFAB_SCHEMA_VERSION);
    assert!(matches!(
        migrated.root.components[0],
        ComponentRecord::Transform {
            translation: [3.0, 2.0, 1.0],
            ..
        }
    ));
}

#[test]
fn schema_v1_rejects_duplicate_legacy_and_component_transforms() {
    let source = r#"(
        schema_version: 1,
        root: (
            instance_id: 7,
            name: "Legacy",
            transform: Some((translation: (0.0, 0.0, 0.0), rotation: (0.0, 0.0, 0.0, 1.0), scale: (1.0, 1.0, 1.0))),
            components: [Transform(translation: (1.0, 0.0, 0.0), rotation_xyzw: (0.0, 0.0, 0.0, 1.0), scale: (1.0, 1.0, 1.0))],
        ),
    )"#;

    assert_eq!(
        PrefabDocument::from_ron(source),
        Err(PrefabError::DuplicateTransform(7))
    );
}

#[test]
fn validation_rejects_duplicate_ids_missing_transform_and_duplicate_components() {
    let mut invalid = document();
    invalid.root.children[0].instance_id = invalid.root.instance_id;
    assert_eq!(
        invalid.validate(),
        Err(PrefabError::DuplicateInstanceId(10))
    );

    let mut invalid = document();
    invalid
        .root
        .components
        .retain(|record| record.kind() != ComponentKind::Transform);
    assert_eq!(invalid.validate(), Err(PrefabError::MissingTransform(10)));

    let mut invalid = document();
    invalid.root.components.push(transform(1.0));
    assert_eq!(invalid.validate(), Err(PrefabError::DuplicateTransform(10)));
}

#[test]
fn typed_patches_override_prefab_values_and_apply_to_node_name() {
    let mut effective = document();
    let overrides = PrefabOverrides {
        patches: vec![
            ComponentPatch::Visibility {
                instance_id: 11,
                visible: true,
            },
            ComponentPatch::TransformTranslation {
                instance_id: 11,
                value: [4.0, 5.0, 6.0],
            },
            ComponentPatch::NameValue {
                instance_id: 11,
                value: "EditedChild".into(),
            },
        ],
        removed_components: vec![],
    };

    overrides.apply_to(&mut effective).unwrap();

    assert_eq!(effective.root.children[0].name, "EditedChild");
    assert!(matches!(
        effective.root.children[0].components[0],
        ComponentRecord::Transform {
            translation: [4.0, 5.0, 6.0],
            ..
        }
    ));
    assert!(matches!(
        effective.root.children[0].components[1],
        ComponentRecord::Visibility { visible: true }
    ));
    assert_eq!(document().root.children[0].name, "Child");
}

#[test]
fn duplicate_patches_and_patch_remove_conflicts_fail_without_mutating_document() {
    let original = document();
    let mut effective = original.clone();
    let duplicate = PrefabOverrides {
        patches: vec![
            ComponentPatch::Visibility {
                instance_id: 11,
                visible: true,
            },
            ComponentPatch::Visibility {
                instance_id: 11,
                visible: false,
            },
        ],
        removed_components: vec![],
    };
    assert!(matches!(
        duplicate.apply_to(&mut effective),
        Err(PrefabError::DuplicatePatch {
            instance_id: 11,
            ..
        })
    ));
    assert_eq!(effective, original);

    let conflict = PrefabOverrides {
        patches: vec![ComponentPatch::TransformScale {
            instance_id: 11,
            value: [2.0; 3],
        }],
        removed_components: vec![RemovedComponent {
            instance_id: 11,
            component: PrefabComponentKind::Transform,
        }],
    };
    assert!(matches!(
        conflict.apply_to(&mut effective),
        Err(PrefabError::PatchAndRemoveConflict { .. })
    ));
    assert_eq!(effective, original);
}

#[test]
fn removed_components_are_instance_local_and_transform_removal_is_rejected() {
    let mut effective = document();
    PrefabOverrides {
        patches: vec![],
        removed_components: vec![RemovedComponent {
            instance_id: 11,
            component: PrefabComponentKind::Visibility,
        }],
    }
    .apply_to(&mut effective)
    .unwrap();
    assert!(!effective.root.children[0]
        .components
        .iter()
        .any(|record| matches!(record, ComponentRecord::Visibility { .. })));
    assert!(document().root.children[0]
        .components
        .iter()
        .any(|record| matches!(record, ComponentRecord::Visibility { .. })));

    let mut unchanged = document();
    let error = PrefabOverrides {
        patches: vec![],
        removed_components: vec![RemovedComponent {
            instance_id: 10,
            component: PrefabComponentKind::Transform,
        }],
    }
    .apply_to(&mut unchanged);
    assert_eq!(error, Err(PrefabError::RemoveTransform(10)));
    assert_eq!(unchanged, document());
}

#[test]
fn prefab_asset_save_and_load_roundtrip() {
    let root = std::env::temp_dir().join(format!("aether-prefab-save-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let path = root.join("test.ron");

    document().save(&path).unwrap();
    let asset = PrefabAsset::load(&path).unwrap();

    assert_eq!(asset.document, document());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn instantiate_remaps_runtime_entities_and_retains_hierarchy_and_overrides() {
    let mut world = World::new();
    let mut assets = TestAssets::default();
    let mut first_instance = instance(100);
    first_instance
        .overrides
        .patches
        .push(ComponentPatch::Visibility {
            instance_id: 11,
            visible: true,
        });
    first_instance
        .overrides
        .removed_components
        .push(RemovedComponent {
            instance_id: 11,
            component: PrefabComponentKind::Name,
        });
    let mut first_context = PrefabContext::new(&first_instance, &mut assets);
    let first = instantiate(&document(), &mut first_context, &mut world).unwrap();
    let second_instance = instance(101);
    let mut second_context = PrefabContext::new(&second_instance, &mut assets);
    let second = instantiate(&document(), &mut second_context, &mut world).unwrap();

    assert_ne!(first[&10], second[&10]);
    assert_eq!(world.len(), 4);
    assert_eq!(
        world
            .query_one::<&PrefabNodeInstance>(first[&11])
            .get()
            .unwrap()
            .parent_instance_id,
        Some(10)
    );
    assert!(world.query_one::<&Visibility>(first[&11]).get().unwrap().0);
    assert!(world.query_one::<&Name>(first[&11]).get().is_err());
    assert!(world.query_one::<&Name>(second[&11]).get().is_ok());
    assert_eq!(
        world
            .query_one::<&PrefabInstanceRoot>(first[&10])
            .get()
            .unwrap()
            .0,
        first_instance
    );
}

#[test]
fn prefab_dependency_failure_does_not_mutate_existing_world() {
    let mut document = document();
    document.root.children[0]
        .components
        .push(ComponentRecord::Mesh {
            source: MeshRef::Builtin("cube".into()),
        });
    let mut world = World::new();
    world.spawn((Name("Existing".into()),));
    let mut assets = TestAssets { fail_mesh: true };
    let config = instance(200);
    let mut context = PrefabContext::new(&config, &mut assets);

    assert!(matches!(
        instantiate(&document, &mut context, &mut world),
        Err(PrefabError::Dependency(_))
    ));
    assert_eq!(world.len(), 1);
    assert_eq!(world.query::<&Name>().iter().next().unwrap().0, "Existing");
}

#[test]
fn scene_instance_config_roundtrips_with_typed_overrides() {
    let config = instance(300);
    let serialized = ron::to_string(&config).unwrap();
    let parsed: PrefabInstanceConfig = ron::from_str(&serialized).unwrap();
    assert_eq!(parsed, config);
}

#[test]
fn patch_requires_matching_component_and_known_node() {
    let mut effective = document();
    let invalid = PrefabOverrides {
        patches: vec![ComponentPatch::MaterialConfig {
            instance_id: 11,
            config: MaterialConfig::default(),
        }],
        removed_components: vec![],
    };
    assert_eq!(
        invalid.apply_to(&mut effective),
        Err(PrefabError::InvalidPatch {
            instance_id: 11,
            component: PrefabComponentKind::Material,
        })
    );
    assert_eq!(effective, document());

    let missing = PrefabOverrides {
        patches: vec![ComponentPatch::LightConfig {
            instance_id: 999,
            config: LightConfig::default(),
        }],
        removed_components: vec![],
    };
    assert!(matches!(
        missing.apply_to(&mut effective),
        Err(PrefabError::InvalidPatch {
            instance_id: 999,
            ..
        })
    ));
}

#[test]
fn asset_loader_rejects_unknown_schema_versions() {
    let source = "(schema_version: 99, root: ())";
    assert_eq!(
        PrefabDocument::from_ron(source),
        Err(PrefabError::SchemaVersion(99))
    );
}

#[path = "tests/acceptance.rs"]
mod acceptance;
