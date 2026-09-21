use super::error::EditorError;
use crate::ecs::components::{
    Atmosphere, Camera, Clouds, Light, MeshHandle, MeshSource, Name, Transform, Visibility,
};
use crate::ecs::{Entity, World};
use crate::renderer::renderable::MaterialUniform;
use crate::scene::{
    AtmosphereConfig, CameraConfig, CloudConfig, LightConfig, MaterialConfig, MeshRef,
};
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Version of the closed editor snapshot and operation schema.
pub const EDITOR_SCHEMA_VERSION: u32 = 2;

/// Serializable editor component whitelist.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ComponentRecord {
    /// Transform data without runtime math types.
    Transform {
        /// Translation [x, y, z].
        translation: [f32; 3],
        /// Quaternion [x, y, z, w].
        rotation_xyzw: [f32; 4],
        /// Scale [x, y, z].
        scale: [f32; 3],
    },
    /// Mesh source without a GPU handle.
    Mesh {
        /// Source reference.
        source: MeshRef,
    },
    /// PBR material configuration.
    Material {
        /// Material values.
        config: MaterialConfig,
    },
    /// Render visibility.
    Visibility {
        /// Whether the entity is rendered.
        visible: bool,
    },
    /// Stable editor name.
    Name {
        /// Name value.
        value: String,
    },
    /// Light configuration.
    Light {
        /// Light values.
        config: LightConfig,
    },
    /// Camera configuration.
    Camera {
        /// Camera values.
        config: CameraConfig,
    },
    /// Atmosphere configuration.
    Atmosphere {
        /// Atmosphere values.
        config: AtmosphereConfig,
    },
    /// Volumetric cloud configuration.
    Clouds {
        /// Cloud values.
        config: CloudConfig,
    },
}

impl ComponentRecord {
    /// Return the closed component kind used by validation and diagnostics.
    pub fn kind(&self) -> super::ComponentKind {
        match self {
            Self::Transform { .. } => super::ComponentKind::Transform,
            Self::Mesh { .. } => super::ComponentKind::Mesh,
            Self::Material { .. } => super::ComponentKind::Material,
            Self::Visibility { .. } => super::ComponentKind::Visibility,
            Self::Name { .. } => super::ComponentKind::Name,
            Self::Light { .. } => super::ComponentKind::Light,
            Self::Camera { .. } => super::ComponentKind::Camera,
            Self::Atmosphere { .. } => super::ComponentKind::Atmosphere,
            Self::Clouds { .. } => super::ComponentKind::Clouds,
        }
    }
}

/// Serializable snapshot of one editor entity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EditorEntitySnapshot {
    /// Snapshot schema version.
    pub schema_version: u32,
    /// Stable editor name; runtime entity bits are intentionally absent.
    pub entity_name: String,
    /// Components in deterministic whitelist order.
    pub records: Vec<ComponentRecord>,
}

/// Capture the T1 whitelist from one ECS entity.
pub fn capture_snapshot(
    world: &World,
    entity: Entity,
) -> Result<EditorEntitySnapshot, EditorError> {
    if !world.contains(entity) {
        return Err(EditorError::MissingEntity(entity));
    }

    let name = world
        .query_one::<&Name>(entity)
        .get()
        .map(|value| value.0.clone())
        .unwrap_or_default();
    let mut records = Vec::new();

    if let Ok(transform) = world.query_one::<&Transform>(entity).get() {
        records.push(ComponentRecord::Transform {
            translation: transform.translation.to_array(),
            rotation_xyzw: transform.rotation.to_array(),
            scale: transform.scale.to_array(),
        });
    }
    if let Ok(mesh) = world.query_one::<&MeshHandle>(entity).get() {
        let source = match &mesh.source {
            MeshSource::Builtin(value) => MeshRef::Builtin(value.clone()),
            MeshSource::File(value) => MeshRef::File(value.clone()),
        };
        records.push(ComponentRecord::Mesh { source });
    }
    if let Ok(material) = world.query_one::<&MaterialUniform>(entity).get() {
        records.push(ComponentRecord::Material {
            config: MaterialConfig {
                albedo: material.albedo,
                roughness: material.roughness,
                metallic: material.metallic,
                unlit: material.unlit != 0,
                albedo_texture: None,
                ..MaterialConfig::default()
            },
        });
    }
    if let Ok(visibility) = world.query_one::<&Visibility>(entity).get() {
        records.push(ComponentRecord::Visibility {
            visible: visibility.0,
        });
    }
    if let Ok(name_component) = world.query_one::<&Name>(entity).get() {
        records.push(ComponentRecord::Name {
            value: name_component.0.clone(),
        });
    }
    if let Ok(light) = world.query_one::<&Light>(entity).get() {
        let direction = world
            .query_one::<&Transform>(entity)
            .get()
            .map(|transform| (transform.rotation * Vec3::NEG_Y).normalize().to_array())
            .unwrap_or([0.0, -1.0, 0.0]);
        records.push(ComponentRecord::Light {
            config: LightConfig {
                light_type: light.light_type,
                direction,
                position: world
                    .query_one::<&Transform>(entity)
                    .get()
                    .map(|transform| transform.translation.to_array())
                    .unwrap_or([0.0; 3]),
                color: light.color,
                intensity: light.intensity,
                range: light.range,
                inner_cone_angle: light.inner_cone_angle,
                outer_cone_angle: light.outer_cone_angle,
            },
        });
    }
    if let Ok(camera) = world.query_one::<&Camera>(entity).get() {
        let (position, yaw, pitch) = world
            .query_one::<&Transform>(entity)
            .get()
            .map(|transform| {
                let (yaw, pitch, _) = transform.rotation.to_euler(glam::EulerRot::YXZ);
                (transform.translation.to_array(), yaw, pitch)
            })
            .unwrap_or(([0.0; 3], 0.0, 0.0));
        records.push(ComponentRecord::Camera {
            config: CameraConfig {
                position,
                yaw,
                pitch,
                speed: camera.speed,
                fov: camera.fov.to_degrees(),
                near: camera.near,
                far: camera.far,
            },
        });
    }
    if let Ok(atmosphere) = world.query_one::<&Atmosphere>(entity).get() {
        records.push(ComponentRecord::Atmosphere {
            config: atmosphere.config.clone(),
        });
    }
    if let Ok(clouds) = world.query_one::<&Clouds>(entity).get() {
        records.push(ComponentRecord::Clouds {
            config: clouds.config.clone(),
        });
    }

    Ok(EditorEntitySnapshot {
        schema_version: EDITOR_SCHEMA_VERSION,
        entity_name: name,
        records,
    })
}
