use crate::asset::{Asset, AssetKind};
use std::path::Path;

/// CPU-owned glTF document metadata for typed asset and skin consumers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GltfDocumentAsset {
    /// Node references, mesh/skin assignments, and child indices.
    pub nodes: Vec<GltfNodeMetadata>,
    /// Mesh names and static primitive counts.
    pub meshes: Vec<GltfMeshMetadata>,
    /// Skin joint and optional skeleton node indices.
    pub skins: Vec<GltfSkinMetadata>,
}

/// Stable document metadata for one glTF node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GltfNodeMetadata {
    /// Optional authored name.
    pub name: Option<String>,
    /// Referenced mesh index for static or skinned geometry.
    pub mesh: Option<usize>,
    /// Referenced skin index when this node is skinned.
    pub skin: Option<usize>,
    /// Child node indices in document order.
    pub children: Vec<usize>,
}

/// Static mesh metadata needed before decoding vertex streams.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GltfMeshMetadata {
    /// Optional authored name.
    pub name: Option<String>,
    /// Number of primitives in the mesh.
    pub primitive_count: usize,
}

/// Skin joint references needed to build a later skeleton asset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GltfSkinMetadata {
    /// Optional authored name.
    pub name: Option<String>,
    /// Node indices that form the skin's joint palette.
    pub joints: Vec<usize>,
    /// Optional root joint node index.
    pub skeleton: Option<usize>,
}

impl Asset for GltfDocumentAsset {
    const KIND: AssetKind = AssetKind::GltfDocument;

    fn load(path: &Path) -> anyhow::Result<Self> {
        read_metadata(path)
    }
}

/// Read node, static mesh, and skin references without loading buffers or images.
pub fn read_metadata(path: &Path) -> anyhow::Result<GltfDocumentAsset> {
    let gltf = gltf::Gltf::open(path).map_err(|error| {
        anyhow::anyhow!("Failed to read glTF metadata '{}': {error}", path.display())
    })?;
    let document = gltf.document;
    let nodes = document
        .nodes()
        .map(|node| GltfNodeMetadata {
            name: node.name().map(String::from),
            mesh: node.mesh().map(|mesh| mesh.index()),
            skin: node.skin().map(|skin| skin.index()),
            children: node.children().map(|child| child.index()).collect(),
        })
        .collect();
    let meshes = document
        .meshes()
        .map(|mesh| GltfMeshMetadata {
            name: mesh.name().map(String::from),
            primitive_count: mesh.primitives().count(),
        })
        .collect();
    let skins = document
        .skins()
        .map(|skin| GltfSkinMetadata {
            name: skin.name().map(String::from),
            joints: skin.joints().map(|joint| joint.index()).collect(),
            skeleton: skin.skeleton().map(|node| node.index()),
        })
        .collect();

    Ok(GltfDocumentAsset {
        nodes,
        meshes,
        skins,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(1);

    struct Fixture(std::path::PathBuf);

    impl Fixture {
        fn new() -> Self {
            let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "aether_gltf_metadata_{}_{}.gltf",
                std::process::id(),
                id
            ));
            let document = r#"{
                "asset":{"version":"2.0"},
                "scene":0,
                "scenes":[{"nodes":[0]}],
                "nodes":[
                    {"name":"SkinnedMesh","mesh":0,"skin":0,"children":[1]},
                    {"name":"RootJoint"}
                ],
                "meshes":[{"name":"Triangle","primitives":[{"attributes":{
                    "POSITION":0,"JOINTS_0":1,"WEIGHTS_0":2
                }}]}],
                "skins":[{"name":"Humanoid","joints":[1],"skeleton":1}],
                "buffers":[{"uri":"mesh.bin","byteLength":108}],
                "bufferViews":[
                    {"buffer":0,"byteOffset":0,"byteLength":36},
                    {"buffer":0,"byteOffset":36,"byteLength":24},
                    {"buffer":0,"byteOffset":60,"byteLength":48}
                ],
                "accessors":[
                    {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]},
                    {"bufferView":1,"componentType":5123,"count":3,"type":"VEC4"},
                    {"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"}
                ]
            }"#;
            fs::write(&path, document).unwrap();
            Self(path)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    #[test]
    fn reads_static_mesh_and_skin_metadata_without_loading_buffers() {
        let fixture = Fixture::new();
        let asset = GltfDocumentAsset::load(&fixture.0).unwrap();

        assert_eq!(asset.nodes.len(), 2);
        assert_eq!(asset.nodes[0].mesh, Some(0));
        assert_eq!(asset.nodes[0].skin, Some(0));
        assert_eq!(asset.nodes[0].children, [1]);
        assert_eq!(asset.meshes[0].name.as_deref(), Some("Triangle"));
        assert_eq!(asset.meshes[0].primitive_count, 1);
        assert_eq!(asset.skins[0].name.as_deref(), Some("Humanoid"));
        assert_eq!(asset.skins[0].joints, [1]);
        assert_eq!(asset.skins[0].skeleton, Some(1));
        assert_eq!(GltfDocumentAsset::KIND, AssetKind::GltfDocument);
    }
}
