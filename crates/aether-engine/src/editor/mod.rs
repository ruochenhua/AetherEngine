//! Deterministic editor snapshots, operations, and atomic transactions.

mod error;
mod history;
mod operation;
mod persist;
mod snapshot;

pub use error::{ComponentKind, EditorError};
pub use history::EditorHistory;
pub use operation::{EditorContext, EditorOperation, OperationLog, OperationLogEntry};
pub use snapshot::{
    capture_snapshot, ComponentRecord, EditorEntitySnapshot, EDITOR_SCHEMA_VERSION,
};

#[cfg(test)]
mod tests;
