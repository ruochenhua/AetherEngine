use crate::asset::{Handle, material::Material};
use crate::asset::mesh::GpuMesh;

/// Mesh renderer component.
///
/// Attaches a GPU mesh and material to an entity.
#[derive(Debug)]
pub struct MeshRenderer {
    /// GPU mesh handle.
    pub mesh: Handle<GpuMesh>,
    /// Material.
    pub material: Material,
    /// Whether to cast shadows.
    pub cast_shadow: bool,
    /// Whether to receive shadows.
    pub receive_shadow: bool,
}


impl MeshRenderer {
    /// Create a new mesh renderer.
    pub fn new(mesh: Handle<GpuMesh>, material: Material) -> Self {
        Self {
            mesh,
            material,
            cast_shadow: true,
            receive_shadow: true,
        }
    }
}
