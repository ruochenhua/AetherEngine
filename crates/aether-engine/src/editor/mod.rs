//! Deterministic editor snapshots, operations, and atomic transactions.

mod error;
mod history;
mod operation;
mod persist;
mod snapshot;

pub use error::{ComponentKind, EditorError};
pub use history::EditorHistory;
pub use operation::{
    EditorContext, EditorOperation, OperationLog, OperationLogEntry, PreparedObject,
};
pub use snapshot::{
    capture_snapshot, ComponentRecord, EditorEntitySnapshot, EDITOR_SCHEMA_VERSION,
};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod inverse_tests;

#[cfg(test)]
mod operation_tests;
