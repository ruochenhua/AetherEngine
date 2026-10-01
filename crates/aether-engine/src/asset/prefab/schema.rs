use crate::editor::{ComponentKind, ComponentRecord};
use crate::scene::{MaterialConfig, MeshRef};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Current on-disk Prefab schema version.
pub const PREFAB_SCHEMA_VERSION: u32 = 2;

/// Stable authored prefab tree. Runtime hecs entity identifiers are never stored here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrefabDocument {
    /// On-disk schema version.
    pub schema_version: u32,
    /// Root node, including all descendants.
    pub root: PrefabNode,
}

/// One node in a prefab hierarchy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrefabNode {
    /// Stable id within this prefab document.
    pub instance_id: u64,
    /// Fallback runtime name when no Name record is authored.
    pub name: String,
    /// Closed T1 component records. Each node has exactly one Transform record.
    #[serde(default)]
    pub components: Vec<ComponentRecord>,
    /// Child nodes, serialized in authored order.
    #[serde(default)]
    pub children: Vec<PrefabNode>,
}

/// Closed component kinds supported by prefab removal overrides.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrefabComponentKind {
    /// Transform component; mandatory on every node and cannot be removed.
    Transform,
    /// Mesh source.
    Mesh,
    /// Material configuration.
    Material,
    /// Render visibility.
    Visibility,
    /// Entity name.
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

impl From<ComponentKind> for PrefabComponentKind {
    fn from(value: ComponentKind) -> Self {
        match value {
            ComponentKind::Transform => Self::Transform,
            ComponentKind::Mesh => Self::Mesh,
            ComponentKind::Material => Self::Material,
            ComponentKind::Visibility => Self::Visibility,
            ComponentKind::Name => Self::Name,
            ComponentKind::Light => Self::Light,
            ComponentKind::Camera => Self::Camera,
            ComponentKind::Atmosphere => Self::Atmosphere,
            ComponentKind::Clouds => Self::Clouds,
        }
    }
}

/// Typed, field-specific instance override.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ComponentPatch {
    /// Replace local translation.
    TransformTranslation {
        /// Stable target node id.
        instance_id: u64,
        /// Translation override [x, y, z].
        value: [f32; 3],
    },
    /// Replace local rotation in xyzw order.
    TransformRotation {
        /// Stable target node id.
        instance_id: u64,
        /// Rotation override quaternion [x, y, z, w].
        value: [f32; 4],
    },
    /// Replace local scale.
    TransformScale {
        /// Stable target node id.
        instance_id: u64,
        /// Scale override [x, y, z].
        value: [f32; 3],
    },
    /// Replace mesh source.
    MeshSource {
        /// Stable target node id.
        instance_id: u64,
        /// Mesh reference override.
        source: MeshRef,
    },
    /// Replace all material values.
    MaterialConfig {
        /// Stable target node id.
        instance_id: u64,
        /// Complete material override.
        config: MaterialConfig,
    },
    /// Replace render visibility.
    Visibility {
        /// Stable target node id.
        instance_id: u64,
        /// Whether the instance is rendered.
        visible: bool,
    },
    /// Replace the runtime name.
    NameValue {
        /// Stable target node id.
        instance_id: u64,
        /// Runtime entity name override.
        value: String,
    },
    /// Replace light values.
    LightConfig {
        /// Stable target node id.
        instance_id: u64,
        /// Complete light override.
        config: crate::scene::LightConfig,
    },
    /// Replace camera values.
    CameraConfig {
        /// Stable target node id.
        instance_id: u64,
        /// Complete camera override.
        config: crate::scene::CameraConfig,
    },
    /// Replace atmosphere values.
    AtmosphereConfig {
        /// Stable target node id.
        instance_id: u64,
        /// Complete atmosphere override.
        config: crate::scene::AtmosphereConfig,
    },
    /// Replace cloud values.
    CloudsConfig {
        /// Stable target node id.
        instance_id: u64,
        /// Complete cloud override.
        config: crate::scene::CloudConfig,
    },
}

impl ComponentPatch {
    pub(super) fn target(&self) -> (u64, PatchField) {
        match self {
            Self::TransformTranslation { instance_id, .. } => {
                (*instance_id, PatchField::Translation)
            }
            Self::TransformRotation { instance_id, .. } => (*instance_id, PatchField::Rotation),
            Self::TransformScale { instance_id, .. } => (*instance_id, PatchField::Scale),
            Self::MeshSource { instance_id, .. } => (*instance_id, PatchField::Mesh),
            Self::MaterialConfig { instance_id, .. } => (*instance_id, PatchField::Material),
            Self::Visibility { instance_id, .. } => (*instance_id, PatchField::Visibility),
            Self::NameValue { instance_id, .. } => (*instance_id, PatchField::Name),
            Self::LightConfig { instance_id, .. } => (*instance_id, PatchField::Light),
            Self::CameraConfig { instance_id, .. } => (*instance_id, PatchField::Camera),
            Self::AtmosphereConfig { instance_id, .. } => (*instance_id, PatchField::Atmosphere),
            Self::CloudsConfig { instance_id, .. } => (*instance_id, PatchField::Clouds),
        }
    }

    pub(super) fn component(&self) -> PrefabComponentKind {
        match self {
            Self::TransformTranslation { .. }
            | Self::TransformRotation { .. }
            | Self::TransformScale { .. } => PrefabComponentKind::Transform,
            Self::MeshSource { .. } => PrefabComponentKind::Mesh,
            Self::MaterialConfig { .. } => PrefabComponentKind::Material,
            Self::Visibility { .. } => PrefabComponentKind::Visibility,
            Self::NameValue { .. } => PrefabComponentKind::Name,
            Self::LightConfig { .. } => PrefabComponentKind::Light,
            Self::CameraConfig { .. } => PrefabComponentKind::Camera,
            Self::AtmosphereConfig { .. } => PrefabComponentKind::Atmosphere,
            Self::CloudsConfig { .. } => PrefabComponentKind::Clouds,
        }
    }
}

/// Component tombstone for an instance. Transform cannot be removed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RemovedComponent {
    /// Target stable node id.
    pub instance_id: u64,
    /// Component omitted from this instance.
    pub component: PrefabComponentKind,
}

/// Per-instance edits. Patches override prefab values; removals are explicit tombstones.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrefabOverrides {
    /// Typed value patches, in deterministic authored order.
    #[serde(default)]
    pub patches: Vec<ComponentPatch>,
    /// Components removed from this instance after applying patches.
    #[serde(default)]
    pub removed_components: Vec<RemovedComponent>,
}

/// Prefab instance reference persisted in a scene.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrefabInstanceConfig {
    /// Stable id of this placed prefab instance within the scene.
    pub instance_id: u64,
    /// Project-relative Prefab asset path.
    pub prefab_asset: String,
    /// Changes and removals applied to this instance.
    #[serde(default)]
    pub overrides: PrefabOverrides,
}

/// Prefab parsing, migration, validation, resolution, or transaction failure.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum PrefabError {
    /// Invalid or unsupported on-disk schema version.
    #[error("unsupported prefab schema version {0}")]
    SchemaVersion(u32),
    /// RON could not be parsed or serialized.
    #[error("prefab serialization failed: {0}")]
    Serialization(String),
    /// A node id occurs more than once.
    #[error("duplicate prefab instance id {0}")]
    DuplicateInstanceId(u64),
    /// A placed prefab instance id is already present in the scene world.
    #[error("duplicate scene prefab instance id {0}")]
    DuplicateSceneInstanceId(u64),
    /// A node has no Transform component.
    #[error("prefab node {0} must contain exactly one Transform component")]
    MissingTransform(u64),
    /// A node contains more than one Transform component.
    #[error("prefab node {0} contains duplicate Transform components")]
    DuplicateTransform(u64),
    /// A node contains a component kind more than once.
    #[error("prefab node {instance_id} contains duplicate component {component:?}")]
    DuplicateComponent {
        /// Node containing the duplicate.
        instance_id: u64,
        /// Repeated component kind.
        component: PrefabComponentKind,
    },
    /// A patch refers to a node or component that does not exist.
    #[error("prefab patch target is missing: node {instance_id}, component {component:?}")]
    InvalidPatch {
        /// Node targeted by the patch or removal.
        instance_id: u64,
        /// Component kind expected by the patch.
        component: PrefabComponentKind,
    },
    /// The same node field was patched twice.
    #[error("duplicate prefab patch for node {instance_id}, field {field}")]
    DuplicatePatch {
        /// Node targeted by the duplicate patches.
        instance_id: u64,
        /// Stable field name.
        field: &'static str,
    },
    /// A component is both patched and removed.
    #[error("prefab component {component:?} on node {instance_id} is both patched and removed")]
    PatchAndRemoveConflict {
        /// Node with conflicting operations.
        instance_id: u64,
        /// Conflicting component kind.
        component: PrefabComponentKind,
    },
    /// Transform is mandatory for every prefab node.
    #[error("Transform cannot be removed from prefab node {0}")]
    RemoveTransform(u64),
    /// A transform value is not finite or has an invalid quaternion.
    #[error("invalid transform values on prefab node {0}")]
    InvalidTransform(u64),
    /// A mesh/material or other runtime dependency could not be resolved.
    #[error("prefab dependency resolution failed: {0}")]
    Dependency(String),
    /// A component cannot be converted to runtime ECS data.
    #[error("unsupported prefab component: {0}")]
    UnsupportedComponent(String),
    /// A failed prefab spawn could not be completely rolled back.
    #[error("prefab transaction rollback failed: {0}")]
    RollbackFailed(String),
    /// Atomic file write failed.
    #[error("prefab file write failed at {path}: {message}")]
    Io {
        /// File path involved in the write.
        path: String,
        /// Underlying failure description.
        message: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum PatchField {
    Translation,
    Rotation,
    Scale,
    Mesh,
    Material,
    Visibility,
    Name,
    Light,
    Camera,
    Atmosphere,
    Clouds,
}

impl PatchField {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Translation => "transform.translation",
            Self::Rotation => "transform.rotation",
            Self::Scale => "transform.scale",
            Self::Mesh => "mesh.source",
            Self::Material => "material.config",
            Self::Visibility => "visibility.value",
            Self::Name => "name.value",
            Self::Light => "light.config",
            Self::Camera => "camera.config",
            Self::Atmosphere => "atmosphere.config",
            Self::Clouds => "clouds.config",
        }
    }
}
