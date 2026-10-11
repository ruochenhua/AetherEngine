use super::*;
use crate::asset::gltf_document::JointWeights;

#[test]
fn skinned_vertex_layout_matches_four_joint_shader_attributes() {
    let mesh = CpuMesh::cube();
    let weights = vec![JointWeights::validated([2, 7, 0, 0], [0.25, 0.75, 0.0, 0.0]).unwrap(); 24];
    let vertices = mesh.to_skinned_vertices(&weights).unwrap();
    assert_eq!(vertices[0].joints, [2, 7, 0, 0]);
    assert_eq!(vertices[0].weights, [0.25, 0.75, 0.0, 0.0]);
    assert_eq!(std::mem::size_of::<Vertex>(), 80);
    assert_eq!(Vertex::desc().array_stride, 80);
    assert!(mesh.to_skinned_vertices(&weights[..23]).is_none());
}

#[test]
fn static_vertex_conversion_disables_skinning_with_zero_weights() {
    let vertex = CpuMesh::cube().to_vertices()[0];
    assert_eq!(vertex.joints, [0; 4]);
    assert_eq!(vertex.weights, [0.0; 4]);
}

#[test]
fn vertex_conversion_generates_missing_normals_for_gltf_meshes() {
    let mut mesh = CpuMesh::cube();
    mesh.normals.clear();
    let vertices = mesh.to_vertices();
    assert_eq!(vertices.len(), mesh.positions.len());
    assert!(vertices.iter().all(|vertex| {
        vertex.normal.iter().all(|component| component.is_finite())
            && (glam::Vec3::from_array(vertex.normal).length() - 1.0).abs() < 1.0e-6
    }));
}
