use super::{EditorOperation, OperationLogEntry, EDITOR_SCHEMA_VERSION};
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
