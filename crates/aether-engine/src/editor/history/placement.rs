use super::{super::operation::PreparedObject, EditorHistory, HistoryEntry};
use crate::editor::{
    error::EditorError,
    operation::{EditorContext, EditorOperation, Inverse, OperationLog},
    persist::persist,
};

impl EditorHistory {
    /// Apply a placement operation after the launcher has resolved its runtime assets.
    pub fn apply_prepared(
        &mut self,
        ctx: &mut EditorContext<'_>,
        operation: EditorOperation,
        prepared: PreparedObject,
    ) -> Result<(), EditorError> {
        let EditorOperation::PlaceObject { object } = &operation else {
            return Err(EditorError::InvalidObjectOperation(
                "prepared runtime data requires a PlaceObject operation".into(),
            ));
        };
        if object != &prepared.object {
            return Err(EditorError::InvalidObjectOperation(
                "prepared object differs from the serialized operation".into(),
            ));
        }

        let entity = prepared.spawn_into(ctx.world)?;
        let rollback = Inverse::Delete { entity };
        let next_log = self.log.appended(operation.clone());
        if let Err(error) = persist(ctx, &next_log) {
            super::rollback_operation(ctx.world, rollback).map_err(EditorError::RollbackFailed)?;
            return Err(error);
        }

        super::select_entity(ctx.world, entity);
        self.log = next_log;
        self.undo.push(HistoryEntry {
            inverse: Inverse::Delete { entity },
            operation,
        });
        self.redo.clear();
        Ok(())
    }

    /// Replay operations that include placements using a host-provided asset resolver.
    pub fn replay_with_prepared<F>(
        &mut self,
        ctx: &mut EditorContext<'_>,
        log: &OperationLog,
        mut prepare: F,
    ) -> Result<(), EditorError>
    where
        F: FnMut(&crate::scene::ObjectConfig) -> Result<PreparedObject, EditorError>,
    {
        for entry in log.entries() {
            match &entry.operation {
                EditorOperation::PlaceObject { object } => {
                    let prepared = prepare(object)?;
                    self.apply_prepared(ctx, entry.operation.clone(), prepared)?;
                }
                operation => self.apply(ctx, operation.clone())?,
            }
        }
        Ok(())
    }
}
