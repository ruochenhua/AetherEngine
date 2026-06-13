//! Terrain rendering subsystem.
//!
//! Generates chunked LOD geometry and manages chunk culling for `TerrainPass`.

pub mod geometry;
pub mod lod;

pub use geometry::{generate_chunk_lod_meshes, height_function_from_source, ProceduralHeight};
pub use lod::{build_chunk_grid, cull_and_select_lod, Chunk};
