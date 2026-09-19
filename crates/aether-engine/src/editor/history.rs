use super::error::{ComponentKind, EditorError};
use super::operation::{EditorContext, EditorOperation, Inverse, OperationLog, RuntimeEntity};
use super::persist::persist;
use super::snapshot::{capture_snapshot, ComponentRecord};
use crate::ecs::components::{
    Atmosphere, Camera, Clouds, Light, MeshHandle, Name, Transform, Visibility,
};
use crate::ecs::{Entity, World};
use crate::renderer::renderable::MaterialUniform;
use glam::{Quat, Vec3};

struct HistoryEntry {
    inverse: Inverse,
}

/// Undo/redo owner for editor transactions.
#[derive(Default)]
pub struct EditorHistory {
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    log: OperationLog,
}

impl EditorHistory {
    /// Apply, persist, and record one editor operation.
    pub fn apply(
        &mut self,
        ctx: &mut EditorContext<'_>,
        operation: EditorOperation,
    ) -> Result<(), EditorError> {
        let (inverse, rollback) = apply_operation(ctx.world, &operation)?;
        let next_log = self.log.appended(operation);
        if let Err(error) = persist(ctx, &next_log) {
            rollback_operation(ctx.world, rollback).map_err(EditorError::RollbackFailed)?;
            return Err(error);
        }
        self.log = next_log;
        self.undo.push(HistoryEntry { inverse });
        self.redo.clear();
        Ok(())
    }

    /// Undo the most recent successful operation and persist the resulting world.
    pub fn undo(&mut self, ctx: &mut EditorContext<'_>) -> Result<(), EditorError> {
        let Some(mut entry) = self.undo.pop() else {
            return Ok(());
        };
        let (next_inverse, rollback) = match apply_inverse(ctx.world, entry.inverse.clone()) {
            Ok(result) => result,
            Err(error) => {
                self.undo.push(entry);
                return Err(error);
            }
        };
        if let Err(error) = persist(ctx, &self.log) {
            rollback_operation(ctx.world, rollback).map_err(EditorError::RollbackFailed)?;
            self.undo.push(entry);
            return Err(error);
        }
        entry.inverse = next_inverse;
        self.redo.push(entry);
        Ok(())
    }

    /// Redo the most recently undone operation and persist the resulting world.
    pub fn redo(&mut self, ctx: &mut EditorContext<'_>) -> Result<(), EditorError> {
        let Some(mut entry) = self.redo.pop() else {
            return Ok(());
        };
        let (next_inverse, rollback) = match apply_inverse(ctx.world, entry.inverse.clone()) {
            Ok(result) => result,
            Err(error) => {
                self.redo.push(entry);
                return Err(error);
            }
        };
        if let Err(error) = persist(ctx, &self.log) {
            rollback_operation(ctx.world, rollback).map_err(EditorError::RollbackFailed)?;
            self.redo.push(entry);
            return Err(error);
        }
        entry.inverse = next_inverse;
        self.undo.push(entry);
        Ok(())
    }

    /// Replay a previously committed operation log against a fresh world.
    pub fn replay(
        &mut self,
        ctx: &mut EditorContext<'_>,
        log: &OperationLog,
    ) -> Result<(), EditorError> {
        for entry in log.entries() {
            self.apply(ctx, entry.operation.clone())?;
        }
        Ok(())
    }

    /// Calculate a deterministic hash of the named T1 snapshot state.
    pub fn state_hash(world: &World) -> Result<String, EditorError> {
        let mut snapshots = world
            .query::<(Entity, &Name)>()
            .iter()
            .map(|(entity, _)| capture_snapshot(world, entity))
            .collect::<Result<Vec<_>, _>>()?;
        snapshots.sort_by(|left, right| left.entity_name.cmp(&right.entity_name));
        let bytes = serde_json::to_vec(&snapshots)
            .map_err(|error| EditorError::Serialization(error.to_string()))?;
        let mut hash = 0xcbf29ce484222325u64;
        for byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        Ok(format!("{hash:016x}"))
    }

    /// Number of undo records.
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// Number of redo records.
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// Borrow the committed replay log.
    pub fn operation_log(&self) -> &OperationLog {
        &self.log
    }
}

fn resolve_name(world: &World, name: &str) -> Result<Entity, EditorError> {
    let matches: Vec<_> = world
        .query::<(Entity, &Name)>()
        .iter()
        .filter_map(|(entity, component)| (component.0 == name).then_some(entity))
        .collect();
    match matches.as_slice() {
        [] => Err(EditorError::EntityNotFound {
            name: name.to_string(),
        }),
        [entity] => Ok(*entity),
        _ => Err(EditorError::DuplicateName {
            name: name.to_string(),
        }),
    }
}

fn validate_new_name(world: &World, name: &str) -> Result<(), EditorError> {
    if name.trim().is_empty() {
        return Err(EditorError::EmptyName);
    }
    if world
        .query::<&Name>()
        .iter()
        .any(|component| component.0 == name)
    {
        return Err(EditorError::DuplicateName {
            name: name.to_string(),
        });
    }
    Ok(())
}

fn has_component(world: &World, entity: Entity, kind: ComponentKind) -> bool {
    match kind {
        ComponentKind::Transform => world.query_one::<&Transform>(entity).get().is_ok(),
        ComponentKind::Mesh => world.query_one::<&MeshHandle>(entity).get().is_ok(),
        ComponentKind::Material => world.query_one::<&MaterialUniform>(entity).get().is_ok(),
        ComponentKind::Visibility => world.query_one::<&Visibility>(entity).get().is_ok(),
        ComponentKind::Name => world.query_one::<&Name>(entity).get().is_ok(),
        ComponentKind::Light => world.query_one::<&Light>(entity).get().is_ok(),
        ComponentKind::Camera => world.query_one::<&Camera>(entity).get().is_ok(),
        ComponentKind::Atmosphere => world.query_one::<&Atmosphere>(entity).get().is_ok(),
        ComponentKind::Clouds => world.query_one::<&Clouds>(entity).get().is_ok(),
    }
}

fn apply_operation(
    world: &mut World,
    operation: &EditorOperation,
) -> Result<(Inverse, Inverse), EditorError> {
    match operation {
        EditorOperation::Rename {
            entity_name,
            new_name,
        } => {
            let entity = resolve_name(world, entity_name)?;
            validate_new_name(world, new_name)?;
            let old_name = world
                .query_one::<&Name>(entity)
                .get()
                .map_err(|_| EditorError::MissingEntity(entity))?
                .0
                .clone();
            world
                .query_one_mut::<&mut Name>(entity)
                .map_err(|_| EditorError::MissingEntity(entity))?
                .0 = new_name.clone();
            Ok((
                Inverse::Rename {
                    entity,
                    name: old_name.clone(),
                },
                Inverse::Rename {
                    entity,
                    name: old_name,
                },
            ))
        }
        EditorOperation::Copy {
            source_name,
            new_name,
        } => {
            let source = resolve_name(world, source_name)?;
            validate_new_name(world, new_name)?;
            let mut state = RuntimeEntity::capture(world, source)?;
            state.name = Some(Name(new_name.clone()));
            let copy = state.spawn_into(world)?;
            Ok((
                Inverse::Delete { entity: copy },
                Inverse::Delete { entity: copy },
            ))
        }
        EditorOperation::Delete { entity_name } => {
            let entity = resolve_name(world, entity_name)?;
            let state = RuntimeEntity::capture(world, entity)?;
            world
                .despawn(entity)
                .map_err(|_| EditorError::MissingEntity(entity))?;
            Ok((
                Inverse::Restore {
                    state: Box::new(state.clone()),
                },
                Inverse::Restore {
                    state: Box::new(state),
                },
            ))
        }
        EditorOperation::AddComponent {
            entity_name,
            component,
        } => {
            let entity = resolve_name(world, entity_name)?;
            let kind = component.kind();
            if has_component(world, entity, kind) {
                return Err(EditorError::DuplicateComponent {
                    entity_name: entity_name.clone(),
                    component: kind,
                });
            }
            insert_component(world, entity, component)?;
            Ok((
                Inverse::Remove { entity, kind },
                Inverse::Remove { entity, kind },
            ))
        }
    }
}

fn insert_component(
    world: &mut World,
    entity: Entity,
    component: &ComponentRecord,
) -> Result<(), EditorError> {
    let result = match component {
        ComponentRecord::Transform {
            translation,
            rotation_xyzw,
            scale,
        } => world.insert(
            entity,
            (Transform {
                translation: Vec3::from_array(*translation),
                rotation: Quat::from_array(*rotation_xyzw),
                scale: Vec3::from_array(*scale),
            },),
        ),
        ComponentRecord::Visibility { visible } => world.insert(entity, (Visibility(*visible),)),
        ComponentRecord::Name { value } => {
            validate_new_name(world, value)?;
            world.insert(entity, (Name(value.clone()),))
        }
        ComponentRecord::Light { config } => world.insert(
            entity,
            (Light {
                light_type: config.light_type,
                color: config.color,
                intensity: config.intensity,
                range: config.range,
                inner_cone_angle: config.inner_cone_angle,
                outer_cone_angle: config.outer_cone_angle,
                cast_shadow: true,
            },),
        ),
        ComponentRecord::Camera { config } => world.insert(
            entity,
            (Camera {
                fov: config.fov.to_radians(),
                near: config.near,
                far: config.far,
                speed: config.speed,
            },),
        ),
        ComponentRecord::Atmosphere { config } => world.insert(
            entity,
            (Atmosphere {
                config: config.clone(),
            },),
        ),
        ComponentRecord::Clouds { config } => world.insert(
            entity,
            (Clouds {
                config: config.clone(),
            },),
        ),
        ComponentRecord::Mesh { .. } => {
            return Err(EditorError::UnsupportedComponent {
                component: ComponentKind::Mesh,
            })
        }
        ComponentRecord::Material { config } => {
            if config.albedo_texture.is_some() {
                return Err(EditorError::UnsupportedComponent {
                    component: ComponentKind::Material,
                });
            }
            world.insert(
                entity,
                (MaterialUniform {
                    albedo: config.albedo,
                    roughness: config.roughness,
                    metallic: config.metallic,
                    unlit: u32::from(config.unlit),
                    _pad: 0,
                    albedo_texture_id: 0,
                },),
            )
        }
    };
    result.map_err(|_| EditorError::MissingEntity(entity))
}

fn apply_inverse(world: &mut World, inverse: Inverse) -> Result<(Inverse, Inverse), EditorError> {
    match inverse {
        Inverse::Rename { entity, name } => {
            let current = world
                .query_one::<&Name>(entity)
                .get()
                .map_err(|_| EditorError::MissingEntity(entity))?
                .0
                .clone();
            world
                .query_one_mut::<&mut Name>(entity)
                .map_err(|_| EditorError::MissingEntity(entity))?
                .0 = name;
            Ok((
                Inverse::Rename {
                    entity,
                    name: current.clone(),
                },
                Inverse::Rename {
                    entity,
                    name: current,
                },
            ))
        }
        Inverse::Delete { entity } => {
            let state = RuntimeEntity::capture(world, entity)?;
            world
                .despawn(entity)
                .map_err(|_| EditorError::MissingEntity(entity))?;
            Ok((
                Inverse::Restore {
                    state: Box::new(state.clone()),
                },
                Inverse::Restore {
                    state: Box::new(state),
                },
            ))
        }
        Inverse::Restore { state } => {
            let entity = state.spawn_into(world)?;
            Ok((Inverse::Delete { entity }, Inverse::Delete { entity }))
        }
        Inverse::Remove { entity, kind } => {
            let record = capture_snapshot(world, entity)?
                .records
                .into_iter()
                .find(|record| record.kind() == kind)
                .ok_or(EditorError::MissingEntity(entity))?;
            remove_component(world, entity, kind)?;
            Ok((
                Inverse::Add {
                    entity,
                    record: record.clone(),
                },
                Inverse::Add { entity, record },
            ))
        }
        Inverse::Add { entity, record } => {
            insert_component(world, entity, &record)?;
            Ok((
                Inverse::Remove {
                    entity,
                    kind: record.kind(),
                },
                Inverse::Remove {
                    entity,
                    kind: record.kind(),
                },
            ))
        }
    }
}

fn remove_component(
    world: &mut World,
    entity: Entity,
    kind: ComponentKind,
) -> Result<(), EditorError> {
    let result = match kind {
        ComponentKind::Transform => world.remove::<(Transform,)>(entity).map(|_| ()),
        ComponentKind::Mesh => world.remove::<(MeshHandle,)>(entity).map(|_| ()),
        ComponentKind::Material => world.remove::<(MaterialUniform,)>(entity).map(|_| ()),
        ComponentKind::Visibility => world.remove::<(Visibility,)>(entity).map(|_| ()),
        ComponentKind::Name => world.remove::<(Name,)>(entity).map(|_| ()),
        ComponentKind::Light => world.remove::<(Light,)>(entity).map(|_| ()),
        ComponentKind::Camera => world.remove::<(Camera,)>(entity).map(|_| ()),
        ComponentKind::Atmosphere => world.remove::<(Atmosphere,)>(entity).map(|_| ()),
        ComponentKind::Clouds => world.remove::<(Clouds,)>(entity).map(|_| ()),
    };
    result.map_err(|_| EditorError::MissingEntity(entity))
}

fn rollback_operation(world: &mut World, inverse: Inverse) -> Result<(), String> {
    apply_inverse(world, inverse)
        .map(|_| ())
        .map_err(|error| error.to_string())
}
