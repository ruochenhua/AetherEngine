use super::error::{ComponentKind, EditorError};
use super::snapshot::{ComponentRecord, EDITOR_SCHEMA_VERSION};
use crate::ecs::components::{
    Atmosphere, Camera, Clouds, Light, MeshHandle, Name, Transform, Visibility,
};
use crate::ecs::{Entity, World};
use crate::renderer::light::LightingUniforms;
use crate::renderer::renderable::MaterialUniform;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A stable-name editor operation suitable for JSONL replay.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum EditorOperation {
    /// Rename one entity.
    Rename {
        /// Existing stable name.
        entity_name: String,
        /// New stable name.
        new_name: String,
    },
    /// Duplicate the closed T1 component set.
    Copy {
        /// Source stable name.
        source_name: String,
        /// New stable name.
        new_name: String,
    },
    /// Delete one entity.
    Delete {
        /// Existing stable name.
        entity_name: String,
    },
    /// Add one whitelisted component when it is absent.
    AddComponent {
        /// Target stable name.
        entity_name: String,
        /// Component value.
        component: ComponentRecord,
    },
}

/// One replayable operation-log entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OperationLogEntry {
    /// Schema version.
    pub schema_version: u32,
    /// Monotonic in-process sequence number.
    pub sequence: u64,
    /// Operation applied by the editor.
    pub operation: EditorOperation,
}

/// In-memory JSONL operation log.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OperationLog {
    pub(super) entries: Vec<OperationLogEntry>,
}

impl OperationLog {
    /// Serialize entries as newline-delimited JSON.
    pub fn to_jsonl(&self) -> Result<String, EditorError> {
        self.entries
            .iter()
            .map(|entry| {
                serde_json::to_string(entry)
                    .map_err(|error| EditorError::Serialization(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|lines| {
                if lines.is_empty() {
                    String::new()
                } else {
                    format!("{}\n", lines.join("\n"))
                }
            })
    }

    /// Parse a JSONL operation log and validate its schema version.
    pub fn from_jsonl(input: &str) -> Result<Self, EditorError> {
        let mut entries = Vec::new();
        for line in input.lines().filter(|line| !line.trim().is_empty()) {
            let entry: OperationLogEntry = serde_json::from_str(line)
                .map_err(|error| EditorError::Serialization(error.to_string()))?;
            if entry.schema_version != EDITOR_SCHEMA_VERSION {
                return Err(EditorError::Serialization(format!(
                    "unsupported editor schema version {}",
                    entry.schema_version
                )));
            }
            entries.push(entry);
        }
        Ok(Self { entries })
    }

    /// Number of operations in the log.
    pub fn apply_count(&self) -> usize {
        self.entries.len()
    }

    /// Borrow the parsed entries for replay or diagnostics.
    pub fn entries(&self) -> &[OperationLogEntry] {
        &self.entries
    }

    pub(super) fn appended(&self, operation: EditorOperation) -> Self {
        let mut next = self.clone();
        next.entries.push(OperationLogEntry {
            schema_version: EDITOR_SCHEMA_VERSION,
            sequence: next.entries.len() as u64,
            operation,
        });
        next
    }
}

/// Borrowed transaction resources owned by the launcher/editor host.
pub struct EditorContext<'a> {
    /// ECS world being edited.
    pub world: &'a mut World,
    /// Current lighting state used by scene serialization.
    pub lighting: &'a LightingUniforms,
    /// Scene name written to the RON document.
    pub scene_name: &'a str,
    /// Atomic scene output path.
    pub save_target: &'a Path,
    /// Atomic JSONL operation-log output path.
    pub operation_log_target: &'a Path,
}

impl<'a> EditorContext<'a> {
    /// Construct an editor transaction context.
    pub fn new(
        world: &'a mut World,
        lighting: &'a LightingUniforms,
        scene_name: &'a str,
        save_target: &'a Path,
        operation_log_target: &'a Path,
    ) -> Self {
        Self {
            world,
            lighting,
            scene_name,
            save_target,
            operation_log_target,
        }
    }
}

#[derive(Clone)]
pub(super) struct RuntimeEntity {
    pub(super) transform: Option<Transform>,
    pub(super) mesh: Option<MeshHandle>,
    pub(super) material: Option<MaterialUniform>,
    pub(super) visibility: Option<Visibility>,
    pub(super) name: Option<Name>,
    pub(super) light: Option<Light>,
    pub(super) camera: Option<Camera>,
    pub(super) atmosphere: Option<Atmosphere>,
    pub(super) clouds: Option<Clouds>,
}

impl RuntimeEntity {
    pub(super) fn capture(world: &World, entity: Entity) -> Result<Self, EditorError> {
        if !world.contains(entity) {
            return Err(EditorError::MissingEntity(entity));
        }
        Ok(Self {
            transform: world.query_one::<&Transform>(entity).get().ok().cloned(),
            mesh: world.query_one::<&MeshHandle>(entity).get().ok().cloned(),
            material: world
                .query_one::<&MaterialUniform>(entity)
                .get()
                .ok()
                .copied(),
            visibility: world.query_one::<&Visibility>(entity).get().ok().copied(),
            name: world.query_one::<&Name>(entity).get().ok().cloned(),
            light: world.query_one::<&Light>(entity).get().ok().cloned(),
            camera: world.query_one::<&Camera>(entity).get().ok().copied(),
            atmosphere: world.query_one::<&Atmosphere>(entity).get().ok().cloned(),
            clouds: world.query_one::<&Clouds>(entity).get().ok().cloned(),
        })
    }

    pub(super) fn spawn_into(&self, world: &mut World) -> Result<Entity, EditorError> {
        let entity = world.spawn(());
        let result = (|| {
            if let Some(value) = &self.transform {
                world
                    .insert(entity, (value.clone(),))
                    .map_err(|error| EditorError::RollbackFailed(error.to_string()))?;
            }
            if let Some(value) = &self.mesh {
                world
                    .insert(entity, (value.clone(),))
                    .map_err(|error| EditorError::RollbackFailed(error.to_string()))?;
            }
            if let Some(value) = self.material {
                world
                    .insert(entity, (value,))
                    .map_err(|error| EditorError::RollbackFailed(error.to_string()))?;
            }
            if let Some(value) = self.visibility {
                world
                    .insert(entity, (value,))
                    .map_err(|error| EditorError::RollbackFailed(error.to_string()))?;
            }
            if let Some(value) = &self.name {
                world
                    .insert(entity, (value.clone(),))
                    .map_err(|error| EditorError::RollbackFailed(error.to_string()))?;
            }
            if let Some(value) = &self.light {
                world
                    .insert(entity, (value.clone(),))
                    .map_err(|error| EditorError::RollbackFailed(error.to_string()))?;
            }
            if let Some(value) = self.camera {
                world
                    .insert(entity, (value,))
                    .map_err(|error| EditorError::RollbackFailed(error.to_string()))?;
            }
            if let Some(value) = &self.atmosphere {
                world
                    .insert(entity, (value.clone(),))
                    .map_err(|error| EditorError::RollbackFailed(error.to_string()))?;
            }
            if let Some(value) = &self.clouds {
                world
                    .insert(entity, (value.clone(),))
                    .map_err(|error| EditorError::RollbackFailed(error.to_string()))?;
            }
            Ok(entity)
        })();
        if result.is_err() {
            let _ = world.despawn(entity);
        }
        result
    }
}

#[derive(Clone)]
pub(super) enum Inverse {
    Rename {
        entity: Entity,
        name: String,
    },
    Delete {
        entity: Entity,
    },
    Restore {
        state: Box<RuntimeEntity>,
    },
    Remove {
        entity: Entity,
        kind: ComponentKind,
    },
    Add {
        entity: Entity,
        record: ComponentRecord,
    },
}
