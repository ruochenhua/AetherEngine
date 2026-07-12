//! Terrain rendering subsystem.
//!
//! Generates chunked LOD geometry and manages chunk culling for `TerrainPass`,
//! `ShadowPass`, and `WaterReflectionPass` via a shared `TerrainGeometry` cache.

pub mod geometry;
pub mod geometry_cache;
pub mod lod;
pub mod material;

pub use geometry::{
    generate_chunk_lod_meshes, height_function_from_source, PerlinHeight, ProceduralHeight,
};
pub use geometry_cache::{ChunkInstanceData, TerrainGeometry};
pub use lod::{build_chunk_grid, cull_and_select_lod, Chunk};
pub use material::{create_terrain_material_bind_group, create_terrain_material_bind_group_layout, write_terrain_uniforms, TerrainUniform};
