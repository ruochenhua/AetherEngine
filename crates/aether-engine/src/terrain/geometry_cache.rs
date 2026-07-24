//! Shared terrain geometry cache.
//!
//! `TerrainGeometry` owns the chunked LOD meshes and per-frame instance data
//! for a terrain entity. It is updated once per frame by the launcher and then
//! consumed by `TerrainPass`, `ShadowPass`, and `WaterReflectionPass` so that
//! terrain casts shadows and appears in water reflections without duplicating
//! geometry generation in each pass.

use crate::asset::mesh::GpuMesh;
use crate::ecs::components::Terrain;
use crate::math::{Frustum, Mat4, Vec3};
use crate::renderer::camera::FlyCamera;
use crate::terrain::{
    build_chunk_grid, cull_and_select_lod, generate_chunk_lod_meshes, height_function_from_source,
    Chunk,
};
use std::sync::Arc;

/// Per-instance data for a single terrain chunk.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ChunkInstanceData {
    /// World-space model matrix (column-major).
    pub model_matrix: [[f32; 4]; 4],
    /// Selected LOD level for this chunk.
    pub lod: u32,
    /// Padding to 16-byte alignment.
    pub _pad: [u32; 3],
}

impl ChunkInstanceData {
    /// Describe the instance vertex buffer layout.
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ChunkInstanceData>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 64,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Uint32,
                },
            ],
        }
    }
}

/// Maximum number of terrain chunks that can be drawn in one frame.
const MAX_TERRAIN_CHUNKS: usize = 1024;

/// Shared terrain geometry state.
///
/// Updated once per frame and read by multiple render passes.
pub struct TerrainGeometry {
    chunks: Vec<Chunk>,
    chunk_meshes: Vec<Vec<Arc<GpuMesh>>>,
    visible_chunk_indices: Vec<usize>,
    chunk_instance_data: Vec<ChunkInstanceData>,
    instance_buffer: wgpu::Buffer,
    instance_buffer_capacity: usize,
    last_terrain: Option<Terrain>,
}

impl TerrainGeometry {
    /// Create an empty terrain geometry cache.
    pub fn new(device: &wgpu::Device) -> Self {
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Terrain Instance Buf"),
            size: (MAX_TERRAIN_CHUNKS * std::mem::size_of::<ChunkInstanceData>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            chunks: Vec::new(),
            chunk_meshes: Vec::new(),
            visible_chunk_indices: Vec::new(),
            chunk_instance_data: Vec::new(),
            instance_buffer,
            instance_buffer_capacity: MAX_TERRAIN_CHUNKS,
            last_terrain: None,
        }
    }

    /// Rebuild or update the terrain geometry for the current frame.
    ///
    /// `aspect` is the current viewport aspect ratio used to build the camera
    /// projection matrix for frustum culling.
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: &FlyCamera,
        aspect: f32,
        terrain: &Terrain,
    ) {
        // If the terrain configuration changed, invalidate cached geometry.
        if self.last_terrain.as_ref() != Some(terrain) {
            self.chunks.clear();
            self.chunk_meshes.clear();
            self.last_terrain = Some(terrain.clone());
        }

        // Build chunks and meshes if they don't exist or if terrain config changed.
        if self.chunks.is_empty() {
            let height_fn = height_function_from_source(&terrain.source);
            self.chunks = build_chunk_grid(
                terrain.geometry.extent,
                terrain.geometry.chunk_size as f32,
                terrain.geometry.max_lod,
            );
            self.chunk_meshes = self
                .chunks
                .iter()
                .map(|chunk| {
                    let cpu_meshes = generate_chunk_lod_meshes(
                        terrain.geometry.max_lod,
                        chunk.size,
                        height_fn.as_ref(),
                        (chunk.center.x, chunk.center.z),
                    );
                    cpu_meshes
                        .into_iter()
                        .map(|cpu| Arc::new(GpuMesh::from_cpu(device, &cpu)))
                        .collect()
                })
                .collect();
        }

        // Cull and select LOD against the main camera frustum.
        let view = camera.view_matrix();
        let proj = camera.projection_matrix(aspect);
        let frustum = Frustum::from_view_projection(proj * view);
        let camera_pos = Self::camera_position_from_view(&view);
        let height_estimate = Self::estimate_height_range(&terrain.source);
        self.visible_chunk_indices = cull_and_select_lod(
            &mut self.chunks,
            camera_pos,
            &frustum,
            height_estimate.0,
            height_estimate.1,
            2.0,
        );

        tracing::debug!(
            target: "aether_engine::terrain::geometry_cache",
            "TerrainGeometry::update: chunks={}, visible={}",
            self.chunks.len(),
            self.visible_chunk_indices.len()
        );

        // Build instance data for all chunks so that depth-only passes (shadow,
        // water reflection) can draw every chunk without rebuilding the buffer.
        self.chunk_instance_data.clear();
        for chunk in &self.chunks {
            let model = Mat4::from_translation(chunk.center);
            self.chunk_instance_data.push(ChunkInstanceData {
                model_matrix: model.to_cols_array_2d(),
                lod: chunk.lod,
                _pad: [0; 3],
            });
        }

        // Grow the instance buffer if the total chunk count exceeds capacity.
        if self.chunk_instance_data.len() > self.instance_buffer_capacity {
            let new_capacity = self.chunk_instance_data.len().max(256);
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Terrain Instance Buf"),
                size: (new_capacity * std::mem::size_of::<ChunkInstanceData>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instance_buffer_capacity = new_capacity;
        }

        if !self.chunk_instance_data.is_empty() {
            queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&self.chunk_instance_data),
            );
        }
    }

    /// All terrain chunks.
    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }

    /// Indices of chunks visible to the main camera.
    pub fn visible_chunk_indices(&self) -> &[usize] {
        &self.visible_chunk_indices
    }

    /// GPU meshes for every chunk and LOD level.
    pub fn chunk_meshes(&self) -> &[Vec<Arc<GpuMesh>>] {
        &self.chunk_meshes
    }

    /// GPU vertex buffer containing one `ChunkInstanceData` per visible chunk.
    pub fn instance_buffer(&self) -> &wgpu::Buffer {
        &self.instance_buffer
    }

    /// Per-chunk instance data uploaded to [`Self::instance_buffer`].
    pub fn chunk_instance_data(&self) -> &[ChunkInstanceData] {
        &self.chunk_instance_data
    }

    fn camera_position_from_view(view: &Mat4) -> Vec3 {
        // view = R * T(-eye) => eye = -R^T * view[3].xyz
        let inv = view.inverse();
        inv.transform_point3(Vec3::ZERO)
    }

    fn estimate_height_range(source: &crate::scene::TerrainSource) -> (f32, f32) {
        // Conservative estimate based on the source's configured amplitude.
        let amplitude = match source {
            crate::scene::TerrainSource::Heightmap(_) => 128.0,
            crate::scene::TerrainSource::Procedural { amplitude, .. } => *amplitude,
            crate::scene::TerrainSource::Perlin { amplitude, .. } => *amplitude,
        };
        let half_range = amplitude * 1.5;
        (-half_range, half_range)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::headless_device_queue;
    use crate::asset::terrain_material::TerrainMaterial;
    use crate::ecs::components::Terrain;
    use crate::scene::{TerrainGeometry as TerrainGeometryConfig, TerrainSource};

    fn default_terrain() -> Terrain {
        Terrain {
            source: TerrainSource::Procedural {
                seed: 0,
                frequency: 0.05,
                amplitude: 32.0,
            },
            geometry: TerrainGeometryConfig::default(),
            material: TerrainMaterial::default(),
            splatmap_path: None,
            layer_configs: Vec::new(),
        }
    }

    #[test]
    fn update_generates_chunks() {
        let Some((device, queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let mut geom = TerrainGeometry::new(&device);
        let terrain = default_terrain();
        let camera = FlyCamera::default();

        geom.update(&device, &queue, &camera, 16.0 / 9.0, &terrain);

        assert!(!geom.chunks().is_empty());
        assert_eq!(geom.chunk_meshes().len(), geom.chunks().len());
        assert!(!geom.visible_chunk_indices().is_empty());
        assert_eq!(geom.chunk_instance_data().len(), geom.chunks().len());
    }

    #[test]
    fn terrain_change_rebuilds_geometry() {
        let Some((device, queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let mut geom = TerrainGeometry::new(&device);
        let terrain_a = default_terrain();
        let camera = FlyCamera::default();

        geom.update(&device, &queue, &camera, 16.0 / 9.0, &terrain_a);
        let first_chunk_count = geom.chunks().len();

        let mut terrain_b = terrain_a.clone();
        terrain_b.geometry.extent = terrain_a.geometry.extent * 2.0;
        geom.update(&device, &queue, &camera, 16.0 / 9.0, &terrain_b);

        assert_ne!(geom.chunks().len(), first_chunk_count);
    }

    #[test]
    fn instance_layout_matches_regular_instance_data() {
        // Terrain chunks and regular mesh instances are byte-compatible at the
        // vertex layout level so that depth-only passes can render both with the
        // same pipeline.
        assert_eq!(
            std::mem::size_of::<ChunkInstanceData>(),
            std::mem::size_of::<crate::asset::mesh::InstanceData>()
        );
        let layout = ChunkInstanceData::desc();
        assert_eq!(layout.array_stride, std::mem::size_of::<ChunkInstanceData>() as u64);
    }
}
