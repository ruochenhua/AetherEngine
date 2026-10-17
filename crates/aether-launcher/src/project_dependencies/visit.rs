use super::{ReferenceKind as Kind, SourceLocator as Locator};
use aether_engine::{
    asset::prefab::{ComponentPatch, PrefabDocument, PrefabNode},
    editor::ComponentRecord,
    scene::{MaterialConfig, MeshRef, SceneDescription, TerrainSource},
};
use serde::de::DeserializeOwned;

pub(super) struct AuthoredReference {
    pub locator: Locator,
    pub kind: Kind,
    pub path: String,
    pub material_relative: bool,
}

/// Unknown fields anywhere in an otherwise supported schema may hide future references.
fn strict<T: DeserializeOwned>(source: &str) -> Result<T, String> {
    let mut deserializer = ron::de::Deserializer::from_str(source).map_err(|e| e.to_string())?;
    let mut ignored = Vec::new();
    let result = serde_ignored::deserialize(&mut deserializer, |path| {
        let path = path.to_string();
        ignored.push(path);
    })
    .map_err(|e| e.to_string())?;
    if !ignored.is_empty() {
        return Err(format!("Unsupported schema fields: {}", ignored.join(", ")));
    }
    deserializer.end().map_err(|e| e.to_string())?;
    Ok(result)
}

pub(super) fn document(source: &str) -> Result<Vec<AuthoredReference>, String> {
    let mut references = Vec::new();
    if let Ok(scene) = SceneDescription::from_ron(source) {
        strict::<SceneDescription>(source)?;
        visit_scene(&scene, &mut references);
        return Ok(references);
    }
    if let Ok(prefab) = PrefabDocument::from_ron(source) {
        #[derive(serde::Deserialize)]
        struct Header {
            schema_version: u32,
        }
        let header: Header = ron::from_str(source).map_err(|e| e.to_string())?;
        if header.schema_version == 1 {
            strict::<LegacyDocument>(source)?;
        } else {
            strict::<PrefabDocument>(source)?;
        }
        visit_node(&prefab.root, &mut references);
        return Ok(references);
    }
    if let Ok(material) = strict::<MaterialConfig>(source) {
        visit_material(&material, Locator::MaterialAsset, true, &mut references);
        return Ok(references);
    }
    Err(
        "Malformed, unsupported, or unclassified RON document; dependency coverage is incomplete"
            .into(),
    )
}

fn push(
    out: &mut Vec<AuthoredReference>,
    locator: Locator,
    kind: Kind,
    path: &str,
    material_relative: bool,
) {
    out.push(AuthoredReference {
        locator,
        kind,
        path: path.into(),
        material_relative,
    });
}
fn mesh(mesh: &MeshRef, locator: Locator, out: &mut Vec<AuthoredReference>) {
    if let MeshRef::File(path) = mesh {
        push(out, locator, Kind::Mesh, path, false);
    }
}
fn visit_material(
    material: &MaterialConfig,
    locator: Locator,
    relative: bool,
    out: &mut Vec<AuthoredReference>,
) {
    for (kind, path) in [
        (Kind::AlbedoTexture, material.albedo_texture.as_deref()),
        (Kind::NormalTexture, material.normal_texture.as_deref()),
        (Kind::OrmTexture, material.orm_texture.as_deref()),
        (Kind::EmissiveTexture, material.emissive_texture.as_deref()),
        (
            Kind::TransparentTexture,
            material
                .transparent
                .as_ref()
                .and_then(|m| m.texture.as_deref()),
        ),
    ] {
        if let Some(path) = path {
            push(out, locator.clone(), kind, path, relative);
        }
    }
}
fn visit_node(node: &PrefabNode, out: &mut Vec<AuthoredReference>) {
    for (component_index, component) in node.components.iter().enumerate() {
        let locator = Locator::PrefabNode {
            instance_id: node.instance_id,
            component_index,
        };
        match component {
            ComponentRecord::Mesh { source } => mesh(source, locator, out),
            ComponentRecord::Material { config } => visit_material(config, locator, false, out),
            _ => (),
        }
    }
    for child in &node.children {
        visit_node(child, out);
    }
}
fn visit_scene(scene: &SceneDescription, out: &mut Vec<AuthoredReference>) {
    for (index, object) in scene.objects.iter().enumerate() {
        let locator = Locator::SceneObject {
            index,
            name: object.name.clone(),
        };
        mesh(&object.mesh, locator.clone(), out);
        if let Some(path) = &object.material_asset {
            push(out, locator.clone(), Kind::MaterialAsset, path, false);
        }
        visit_material(&object.material, locator, false, out);
    }
    for (index, instance) in scene.prefab_instances.iter().enumerate() {
        let locator = Locator::ScenePrefab {
            index,
            instance_id: instance.instance_id,
            override_index: None,
            node_id: None,
        };
        push(
            out,
            locator,
            Kind::PrefabAsset,
            &instance.prefab_asset,
            false,
        );
        for (override_index, patch) in instance.overrides.patches.iter().enumerate() {
            match patch {
                ComponentPatch::MeshSource {
                    instance_id,
                    source,
                } => mesh(
                    source,
                    Locator::ScenePrefab {
                        index,
                        instance_id: instance.instance_id,
                        override_index: Some(override_index),
                        node_id: Some(*instance_id),
                    },
                    out,
                ),
                ComponentPatch::MaterialConfig {
                    instance_id,
                    config,
                } => visit_material(
                    config,
                    Locator::ScenePrefab {
                        index,
                        instance_id: instance.instance_id,
                        override_index: Some(override_index),
                        node_id: Some(*instance_id),
                    },
                    false,
                    out,
                ),
                _ => (),
            }
        }
    }
    if let Some(terrain) = &scene.terrain {
        if let TerrainSource::Heightmap(path) = &terrain.source {
            push(
                out,
                Locator::Terrain { layer: None },
                Kind::TerrainHeightmap,
                path,
                false,
            );
        }
        if let Some(path) = &terrain.splatmap {
            push(
                out,
                Locator::Terrain { layer: None },
                Kind::TerrainSplatmap,
                path,
                false,
            );
        }
        for (index, layer) in terrain.layers.iter().enumerate() {
            for (kind, path) in [
                (Kind::TerrainAlbedo, layer.albedo_texture.as_deref()),
                (Kind::TerrainNormal, layer.normal_texture.as_deref()),
                (
                    Kind::TerrainRoughnessMetallic,
                    layer.roughness_metallic_texture.as_deref(),
                ),
            ] {
                if let Some(path) = path {
                    push(
                        out,
                        Locator::Terrain { layer: Some(index) },
                        kind,
                        path,
                        false,
                    );
                }
            }
        }
    }
    if let Some(water) = &scene.water {
        for (kind, path) in [
            (Kind::WaterDudv, water.dudv_map.as_deref()),
            (Kind::WaterNormal, water.normal_map.as_deref()),
        ] {
            if let Some(path) = path {
                push(out, Locator::Water, kind, path, false);
            }
        }
    }
}

pub(super) fn endpoint_diagnostic(extension: &str, bytes: Option<&[u8]>) -> Option<String> {
    match extension {
        "obj" | "mtl" | "fbx" | "dae" | "blend" | "usd" | "usda" | "usdc" | "usdz" => {
            Some(format!("Unvisited authored-reference format: {extension}"))
        }
        "gltf" | "glb" => {
            let bytes = bytes?;
            match gltf::Gltf::from_slice(bytes) {
                Err(error) => Some(format!("Unvisited or malformed glTF document: {error}")),
                Ok(document) => {
                    let external_buffers = document.buffers().any(|buffer| matches!(buffer.source(), gltf::buffer::Source::Uri(uri) if !uri.starts_with("data:")));
                    let external_images = document.images().any(|image| matches!(image.source(), gltf::image::Source::Uri {uri,..} if !uri.starts_with("data:")));
                    (external_buffers || external_images)
                        .then(|| "Unvisited external-resource glTF references".into())
                }
            }
        }
        _ => None,
    }
}

// Shape-only mirror for ignored-field detection after the real v1 migration validator.
// Values remain owned and validated by PrefabDocument::from_ron.
#[derive(serde::Deserialize)]
struct LegacyDocument {
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    #[serde(rename = "root")]
    _root: LegacyNode,
}
#[derive(serde::Deserialize)]
struct LegacyNode {
    #[serde(rename = "instance_id")]
    _instance_id: u64,
    #[serde(rename = "name")]
    _name: String,
    #[serde(default, rename = "transform")]
    _transform: Option<aether_engine::scene::TransformConfig>,
    #[serde(default, rename = "components")]
    _components: Vec<ComponentRecord>,
    #[serde(default, rename = "children")]
    _children: Vec<LegacyNode>,
}
