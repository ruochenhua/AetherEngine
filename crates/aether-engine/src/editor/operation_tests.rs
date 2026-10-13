use super::{
    EditorOperation, MaterialAssignment, OperationLog, OperationLogEntry, EDITOR_SCHEMA_VERSION,
};
use crate::scene::{MaterialConfig, MeshRef, ObjectConfig};

#[test]
fn place_object_operation_serializes_only_stable_scene_data() {
    let operation = EditorOperation::PlaceObject {
        object: ObjectConfig {
            name: "DroppedModel".into(),
            mesh: MeshRef::File("assets/models/rock.obj".into()),
            material: MaterialConfig::default(),
            material_asset: None,
            visible: true,
            transform: Default::default(),
            physics: None,
        },
    };
    let entry = OperationLogEntry {
        schema_version: EDITOR_SCHEMA_VERSION,
        sequence: 0,
        operation: operation.clone(),
    };

    let encoded = serde_json::to_string(&entry).unwrap();
    let decoded: OperationLogEntry = serde_json::from_str(&encoded).unwrap();

    assert_eq!(decoded.operation, operation);
    assert!(encoded.contains("assets/models/rock.obj"));
    assert!(!encoded.contains("gpu"));
    assert!(!encoded.contains("handle"));
}

#[test]
fn material_assignments_jsonl_round_trip_texture_paths_and_asset_path() {
    let inline = MaterialConfig {
        albedo_texture: Some("textures/albedo.png".into()),
        normal_texture: Some("textures/normal.png".into()),
        orm_texture: Some("textures/orm.png".into()),
        emissive_texture: Some("textures/emissive.png".into()),
        ..MaterialConfig::default()
    };
    let entries = [
        OperationLogEntry {
            schema_version: EDITOR_SCHEMA_VERSION,
            sequence: 0,
            operation: EditorOperation::SetMaterial {
                entity_name: "Mesh".into(),
                assignment: MaterialAssignment::Inline {
                    material: inline.clone(),
                },
            },
        },
        OperationLogEntry {
            schema_version: EDITOR_SCHEMA_VERSION,
            sequence: 1,
            operation: EditorOperation::SetMaterial {
                entity_name: "Mesh".into(),
                assignment: MaterialAssignment::Asset {
                    path: "materials/paint.ron".into(),
                },
            },
        },
    ];
    let jsonl = entries
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .join("\n");
    let log = OperationLog::from_jsonl(&jsonl).unwrap();
    let round_trip = OperationLog::from_jsonl(&log.to_jsonl().unwrap()).unwrap();

    assert_eq!(round_trip, log);
    let encoded = log.to_jsonl().unwrap();
    assert!(encoded.contains("textures/albedo.png"));
    assert!(encoded.contains("textures/normal.png"));
    assert!(encoded.contains("textures/orm.png"));
    assert!(encoded.contains("textures/emissive.png"));
    assert!(encoded.contains("materials/paint.ron"));
    assert!(!encoded.contains("texture_id"));
}
