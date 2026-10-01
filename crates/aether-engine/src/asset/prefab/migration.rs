use super::{PrefabDocument, PrefabError, PrefabNode, PREFAB_SCHEMA_VERSION};
use crate::editor::{ComponentKind, ComponentRecord};
use crate::scene::TransformConfig;
use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct PrefabHeader {
    pub(super) schema_version: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LegacyPrefabDocumentV1 {
    schema_version: u32,
    root: LegacyPrefabNodeV1,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyPrefabNodeV1 {
    instance_id: u64,
    name: String,
    #[serde(default)]
    transform: Option<TransformConfig>,
    #[serde(default)]
    components: Vec<ComponentRecord>,
    #[serde(default)]
    children: Vec<LegacyPrefabNodeV1>,
}

pub(super) fn migrate_v1(old: LegacyPrefabDocumentV1) -> Result<PrefabDocument, PrefabError> {
    if old.schema_version != 1 {
        return Err(PrefabError::SchemaVersion(old.schema_version));
    }
    let document = PrefabDocument {
        schema_version: PREFAB_SCHEMA_VERSION,
        root: migrate_v1_node(old.root)?,
    };
    document.validate()?;
    Ok(document)
}

fn migrate_v1_node(old: LegacyPrefabNodeV1) -> Result<PrefabNode, PrefabError> {
    let has_transform = old
        .components
        .iter()
        .any(|record| record.kind() == ComponentKind::Transform);
    if old.transform.is_some() && has_transform {
        return Err(PrefabError::DuplicateTransform(old.instance_id));
    }
    let mut components = old.components;
    if let Some(transform) = old.transform {
        components.insert(
            0,
            ComponentRecord::Transform {
                translation: transform.translation,
                rotation_xyzw: transform.rotation,
                scale: transform.scale,
            },
        );
    }
    Ok(PrefabNode {
        instance_id: old.instance_id,
        name: old.name,
        components,
        children: old
            .children
            .into_iter()
            .map(migrate_v1_node)
            .collect::<Result<_, _>>()?,
    })
}
