use super::super::*;

#[test]
fn owned_gltf_fixture_retains_geometry_skin_and_animation_data() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/t7_owned_document.gltf");
    let document = GltfDocument::load(&path).expect("embedded glTF fixture should load");

    assert_eq!(document.buffers.len(), 1);
    assert_eq!(document.nodes.len(), 2);
    assert_eq!(document.nodes[1].parent, Some(0));
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
