use hecs::Entity;
use thiserror::Error;

/// The closed set of components that can be edited or persisted by T1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentKind {
    /// World-space transform.
    Transform,
    /// Mesh source reference.
    Mesh,
    /// PBR material configuration.
    Material,
    /// Render visibility.
    Visibility,
    /// Human-readable entity name.
    Name,
    /// Light configuration.
    Light,
    /// Camera configuration.
    Camera,
    /// Atmosphere configuration.
    Atmosphere,
    /// Cloud configuration.
    Clouds,
}

impl std::fmt::Display for ComponentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Transform => "Transform",
            Self::Mesh => "Mesh",
            Self::Material => "Material",
            Self::Visibility => "Visibility",
            Self::Name => "Name",
            Self::Light => "Light",
            Self::Camera => "Camera",
            Self::Atmosphere => "Atmosphere",
            Self::Clouds => "Clouds",
        })
    }
}

/// Errors returned by editor validation, persistence, or rollback.
#[derive(Debug, Error)]
pub enum EditorError {
    /// The requested named entity does not exist.
    #[error("entity '{name}' was not found")]
    EntityNotFound {
        /// Requested stable name.
        name: String,
    },
    /// A runtime entity handle is no longer valid.
    #[error("entity {0:?} no longer exists")]
    MissingEntity(Entity),
    /// Two entities would have the same editor name.
    #[error("entity name '{name}' is already in use")]
    DuplicateName {
        /// Conflicting stable name.
        name: String,
    },
    /// A name is empty or contains only whitespace.
    #[error("entity name must not be empty")]
    EmptyName,
    /// An operation would add a component that is already present.
    #[error("entity '{entity_name}' already has component {component}")]
    DuplicateComponent {
        /// Target stable name.
        entity_name: String,
        /// Component that was already present.
        component: ComponentKind,
    },
    /// The closed T1 schema does not support the requested runtime conversion.
    #[error("component {component} is not supported by this editor operation")]
    UnsupportedComponent {
        /// Component that lacks a safe runtime resolver.
        component: ComponentKind,
    },
    /// File-system failure during atomic persistence.
    #[error("I/O error at {path}: {source}")]
    Io {
        /// File path involved in the operation.
        path: String,
        #[source]
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// Scene or operation-log serialization failed.
    #[error("serialization failed: {0}")]
    Serialization(String),
    /// The inverse operation could not restore the world after a failed commit.
    #[error("rollback failed: {0}")]
    RollbackFailed(String),
}
