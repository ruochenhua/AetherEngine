use super::super::*;

#[test]
fn owned_gltf_fixture_retains_geometry_skin_and_animation_data() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/t7_owned_document.gltf");
    let document = GltfDocument::load(&path).expect("embedded glTF fixture should load");

    assert_eq!(document.buffers.len(), 1);
    assert_eq!(document.nodes.len(), 3);
    assert_eq!(document.nodes[1].parent, Some(0));
    assert_eq!(document.nodes[2].mesh, Some(0));
    assert_eq!(document.nodes[2].skin, None);
    let imported = crate::asset::loaders::gltf::load(&path)
        .expect("fixture render mesh should retain source primitive identities");
    assert_eq!(imported.submeshes.len(), 4);
    assert_eq!(
        imported
            .submeshes
            .iter()
            .map(|submesh| (submesh.source_node_index, submesh.source_primitive_index))
            .collect::<Vec<_>>(),
        vec![
            (Some(0), Some(0)),
            (Some(0), Some(1)),
            (Some(2), Some(0)),
            (Some(2), Some(1))
        ]
    );
    assert_eq!(
        document.meshes[0].primitives[0]
            .mesh
            .as_ref()
            .unwrap()
            .positions
            .len(),
        3
    );
    assert_eq!(document.meshes[0].primitives[0].weights.len(), 3);
    assert_eq!(document.skins[0].joints, vec![1]);
    assert_eq!(document.skins[0].inverse_bind_matrices.len(), 1);
    assert_eq!(document.animations[0].channels.len(), 1);
    assert_eq!(
        document.animations[0].channels[0].target,
        AnimationPath::Translation
    );
    assert_eq!(
        document.animations[0].channels[0].times.as_ref(),
        &[0.0, 1.0]
    );
}
