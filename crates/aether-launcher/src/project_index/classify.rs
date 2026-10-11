use super::{ResourceKind, ResourceValidity};
use aether_engine::asset::prefab::PrefabDocument;
use aether_engine::scene::{MaterialConfig, SceneDescription};
use ron::value::Value;
use std::fs;
use std::path::Path;

const MAX_RON_HEADER_BYTES: u64 = 256 * 1024;

pub(super) fn classify(
    path: &Path,
    root_name: &str,
    size: u64,
) -> Option<(ResourceKind, ResourceValidity)> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "ron" => Some(classify_ron(path, root_name, size)),
        "gltf" | "glb" if root_name == "assets" => {
            Some((ResourceKind::Gltf, ResourceValidity::Valid))
        }
        "obj" if root_name == "assets" => Some((ResourceKind::Obj, ResourceValidity::Valid)),
        "png" | "jpg" | "jpeg" | "hdr" if root_name == "assets" => {
            Some((ResourceKind::Image, ResourceValidity::Valid))
        }
        _ => None,
    }
}

fn classify_ron(path: &Path, root_name: &str, size: u64) -> (ResourceKind, ResourceValidity) {
    if size > MAX_RON_HEADER_BYTES {
        return (ResourceKind::Unknown, ResourceValidity::Invalid);
    }
    let Ok(source) = fs::read_to_string(path) else {
        return (ResourceKind::Unknown, ResourceValidity::Invalid);
    };
    if root_name == "scenes" {
        if source.trim().is_empty() {
            return (ResourceKind::Unknown, ResourceValidity::Invalid);
        }
        const SCENE_FIELDS: [&str; 12] = [
            "name",
            "camera",
            "lights",
            "ambient",
            "objects",
            "prefab_instances",
            "terrain",
            "atmosphere",
            "water",
            "clouds",
            "god_ray",
            "particle_emitters",
        ];
        if has_only_fields(&source, &SCENE_FIELDS)
            && has_shape(&source, &["name", "camera"])
            && SceneDescription::from_ron(&source).is_ok()
        {
            return (ResourceKind::Scene, ResourceValidity::Valid);
        }
        return (
            ResourceKind::Unknown,
            if ron::from_str::<Value>(&source).is_err() {
                ResourceValidity::Invalid
            } else {
                ResourceValidity::Unknown
            },
        );
    }
    if PrefabDocument::from_ron(&source).is_ok() {
        return (ResourceKind::Prefab, ResourceValidity::Valid);
    }
    const MATERIAL_EVIDENCE: [&str; 4] = [
        "albedo_texture",
        "normal_texture",
        "orm_texture",
        "emissive_texture",
    ];
    if has_material_evidence(&source, &MATERIAL_EVIDENCE)
        && ron::from_str::<MaterialConfig>(&source).is_ok()
    {
        return (ResourceKind::Material, ResourceValidity::Valid);
    }
    let validity = if ron::from_str::<Value>(&source).is_err() {
        ResourceValidity::Invalid
    } else {
        ResourceValidity::Unknown
    };
    (ResourceKind::Unknown, validity)
}

fn has_shape(source: &str, fields: &[&str]) -> bool {
    ron_map_keys(source).is_some_and(|keys| {
        fields
            .iter()
            .any(|field| keys.iter().any(|key| key == field))
    })
}

fn has_only_fields(source: &str, allowed: &[&str]) -> bool {
    ron_map_keys(source).is_some_and(|keys| keys.iter().all(|key| allowed.contains(&key.as_str())))
}

fn ron_map_keys(source: &str) -> Option<Vec<String>> {
    let Value::Map(map) = ron::from_str::<Value>(source).ok()? else {
        return None;
    };
    Some(
        map.keys()
            .filter_map(|value| match value {
                Value::String(name) => Some(name.clone()),
                _ => None,
            })
            .collect(),
    )
}

fn has_material_evidence(source: &str, fields: &[&str]) -> bool {
    if !has_shape(source, fields) {
        return false;
    }
    fields.iter().any(|field| {
        source
            .find(field)
            .and_then(|offset| source[offset + field.len()..].split_once(':'))
            .is_some_and(|(_, value)| value.trim_start().starts_with("Some("))
    })
}
