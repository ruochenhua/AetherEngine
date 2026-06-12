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
/// Extract phase (ECS → render batches).
pub mod extract;
/// Per-frame data context (RenderFrame).
pub mod frame;
/// Image-Based Lighting (IBL) loader and resources.
pub mod ibl;
/// Transform gizmo rendering and interaction.
pub mod gizmo;
/// CPU ray-casting picking.
pub mod picking;
/// Light component definitions.
pub mod light;
/// Pass trait and type-safe resource handles.
pub mod pass;
/// Shared GPU uniform types (MaterialUniform, ObjectUniform, ...).
pub mod renderable;
/// Render pass implementations.
pub mod passes;
/// Resource type tags (GPosition, GNormal, ...).
pub mod resource;
/// Transient resource table.
pub mod resource_table;
/// Pipeline builder (topological sort, texture allocation).
pub mod pipeline_builder;
/// Pass scheduler and execution.
pub mod scheduler;
