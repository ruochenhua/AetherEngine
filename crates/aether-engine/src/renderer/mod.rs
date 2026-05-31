//! Rendering module.
//!
//! Core rendering system built on wgpu. Features:
//! - Type-safe pass scheduling via PipelineBuilder + Scheduler
//! - Deferred shading pipeline
//! - PBR material support
//! - Extensible pass system

/// Camera types and controllers.
pub mod camera;
/// wgpu context and device management.
pub mod context;
/// RenderGraph pass scheduling (legacy).
pub mod graph;
/// Light component definitions.
pub mod light;
/// Mesh rendering utilities.
pub mod mesh;
/// Pass trait and type-safe resource handles.
pub mod pass;
/// Render pass implementations.
pub mod passes;
/// Resource type tags (GPosition, GNormal, ...).
pub mod resource;
/// Transient resource table.
pub mod resource_table;
/// Pipeline builder and scheduler.
pub mod scheduler;
