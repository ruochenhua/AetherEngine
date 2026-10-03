/// Interleaved PBR vertex carrying four normalized joint influences.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SkinnedVertex {
    /// Object-space position.
    pub position: [f32; 3],
    /// Object-space normal.
    pub normal: [f32; 3],
    /// First texture coordinate set.
    pub uv: [f32; 2],
    /// Tangent direction and handedness.
    pub tangent: [f32; 4],
    /// Palette joint indices.
    pub joints: [u32; 4],
    /// Normalized palette weights.
    pub weights: [f32; 4],
}

impl SkinnedVertex {
    /// Describe the PBR and skin influence attributes for the vertex shader.
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 9,
                    format: wgpu::VertexFormat::Uint32x4,
                },
                wgpu::VertexAttribute {
                    offset: 64,
                    shader_location: 10,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SkinnedVertex;

    #[test]
    fn skinned_vertex_layout_has_aligned_four_joint_attributes() {
        let layout = SkinnedVertex::desc();
        assert_eq!(layout.array_stride, 80);
        assert_eq!(layout.attributes.len(), 6);
        assert_eq!(layout.attributes[4].shader_location, 9);
        assert_eq!(layout.attributes[4].format, wgpu::VertexFormat::Uint32x4);
        assert_eq!(layout.attributes[5].shader_location, 10);
        assert_eq!(layout.attributes[5].format, wgpu::VertexFormat::Float32x4);
        assert_eq!(layout.attributes[5].offset, 64);
    }
}
