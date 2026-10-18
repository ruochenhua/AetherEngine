//! Deterministic editor snapshots, operations, and atomic transactions.

mod error;
mod history;
mod operation;
mod persist;
mod snapshot;
mod world_resource_usage;

pub use error::{ComponentKind, EditorError};
pub use history::EditorHistory;
pub use operation::{
    EditorContext, EditorOperation, MaterialAssignment, OperationLog, OperationLogEntry,
    PreparedMaterial, PreparedObject,
};
pub use snapshot::{
    capture_snapshot, ComponentRecord, EditorEntitySnapshot, EDITOR_SCHEMA_VERSION,
};
pub use world_resource_usage::{
    capture_world_resource_usage, WorldResourceDiagnostic, WorldResourceKind, WorldResourceLocator,
    WorldResourceUse,
};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod inverse_tests;

#[cfg(test)]
mod operation_tests;

#[cfg(test)]
mod material_operation_tests;
