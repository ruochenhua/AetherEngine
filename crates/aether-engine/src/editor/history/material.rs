use super::{super::operation::PreparedMaterial, EditorHistory, HistoryEntry};
use crate::ecs::{Entity, World};
use crate::editor::{
    error::EditorError,
    operation::{
        EditorContext, EditorOperation, Inverse, MaterialAssignment, MaterialRuntimeState,
    },
};

impl EditorHistory {
    /// Apply a material operation after the host has resolved its runtime state.
    pub fn apply_prepared_material(
        &mut self,
        ctx: &mut EditorContext<'_>,
        operation: EditorOperation,
        prepared: PreparedMaterial,
    ) -> Result<(), EditorError> {
        let EditorOperation::SetMaterial {
            entity_name,
            assignment,
        } = &operation
        else {
            return Err(EditorError::InvalidMaterialOperation(
                "prepared material data requires a SetMaterial operation".into(),
            ));
        };
        validate_prepared(assignment, &prepared)?;

        let entity = super::resolve_name(ctx.world, entity_name)?;
        if ctx
            .world
            .query_one::<&crate::ecs::components::MeshHandle>(entity)
            .get()
            .is_err()
        {
            return Err(EditorError::InvalidMaterialOperation(
                "target entity is not a Mesh".into(),
            ));
        }
        let previous = MaterialRuntimeState::capture(ctx.world, entity)?;
        let next = MaterialRuntimeState::from_prepared(prepared);
        install_material_state(ctx.world, entity, &next)?;

        let next_log = self.log.appended(operation.clone());
        if let Err(error) = super::super::persist::persist(ctx, &next_log) {
            install_material_state(ctx.world, entity, &previous)
                .map_err(|rollback| EditorError::RollbackFailed(rollback.to_string()))?;
            return Err(error);
        }

        self.log = next_log;
        self.undo.push(HistoryEntry {
            inverse: Inverse::Material {
                entity_name: entity_name.clone(),
                state: Box::new(previous),
            },
            operation,
        });
        self.redo.clear();
        Ok(())
    }
}

pub(super) fn apply_inverse_material(
    world: &mut World,
    entity_name: String,
    state: Box<MaterialRuntimeState>,
) -> Result<(Inverse, Inverse), EditorError> {
    let entity = super::resolve_name(world, &entity_name)?;
    let current = MaterialRuntimeState::capture(world, entity)?;
    install_material_state(world, entity, &state)?;
    Ok((
        Inverse::Material {
            entity_name: entity_name.clone(),
            state: Box::new(current.clone()),
        },
        Inverse::Material {
            entity_name,
            state: Box::new(current),
        },
    ))
}

pub(super) fn install_material_state(
    world: &mut World,
    entity: Entity,
    state: &MaterialRuntimeState,
) -> Result<(), EditorError> {
    set_optional_component(world, entity, state.material.clone())?;
    set_optional_component(world, entity, state.uniform)?;
    set_optional_component(world, entity, state.extracted_material.clone())?;
    set_optional_component::<crate::ecs::components::MaterialAssetRef>(
        world,
        entity,
        state.material_asset.clone(),
    )?;
    set_optional_component::<crate::ecs::components::MaterialAssetStatus>(
        world,
        entity,
        state.material_asset_status.clone(),
    )?;
    set_optional_component::<crate::renderer::transparent::TransparentMaterial>(
        world,
        entity,
        state.transparent_material.clone(),
    )?;
    Ok(())
}

fn set_optional_component<T: hecs::Component + Clone>(
    world: &mut World,
    entity: Entity,
    value: Option<T>,
) -> Result<(), EditorError> {
    if world.query_one::<&T>(entity).get().is_ok() {
        world
            .remove::<(T,)>(entity)
            .map_err(|error| EditorError::RollbackFailed(error.to_string()))?;
    }
    if let Some(value) = value {
        world
            .insert(entity, (value,))
            .map_err(|error| EditorError::RollbackFailed(error.to_string()))?;
    }
    Ok(())
}

fn validate_prepared(
    assignment: &MaterialAssignment,
    prepared: &PreparedMaterial,
) -> Result<(), EditorError> {
    match assignment {
        MaterialAssignment::Inline { material } => {
            if material != &prepared.material {
                return Err(EditorError::InvalidMaterialOperation(
                    "prepared material differs from the inline operation".into(),
                ));
            }
            if prepared.material_asset.is_some() || prepared.material_asset_status.is_some() {
                return Err(EditorError::InvalidMaterialOperation(
                    "inline assignment cannot carry linked asset state".into(),
                ));
            }
        }
        MaterialAssignment::Asset { path } => {
            if path.trim().is_empty() {
                return Err(EditorError::InvalidMaterialOperation(
                    "material asset path must not be empty".into(),
                ));
            }
            if prepared
                .material_asset
                .as_ref()
                .map(|reference| reference.0.as_str())
                != Some(path.as_str())
            {
                return Err(EditorError::InvalidMaterialOperation(
                    "prepared asset reference differs from the operation path".into(),
                ));
            }
        }
    }
    Ok(())
}
