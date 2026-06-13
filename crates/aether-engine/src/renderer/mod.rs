//! Rendering module.
//!
//! Core rendering system built on wgpu. Features:
//! - Type-safe pass scheduling via PipelineBuilder + Scheduler
//! - Deferred shading pipeline
//! - PBR material support
//! - Extensible pass system

/// Camera types and controllers.
pub mod camera;
/// Volumetric cloud utilities.
pub mod clouds;
/// wgpu context and device management.
pub mod context;
/// Extract phase (ECS → render batches).
pub mod extract;
/// Per-frame data context (RenderFrame).
pub mod frame;
/// Transform gizmo rendering and interaction.
pub mod gizmo;
/// GPU timestamp query timer.
pub mod gpu_timer;
/// Image-Based Lighting (IBL) loader and resources.
pub mod ibl;
/// Light component definitions.
pub mod light;
/// Pass trait and type-safe resource handles.
pub mod pass;
/// Render pass implementations.
pub mod passes;
/// CPU ray-casting picking.
pub mod picking;
/// Pipeline builder (topological sort, texture allocation).
pub mod pipeline_builder;
/// Shared GPU uniform types (MaterialUniform, ObjectUniform, ...).
pub mod renderable;
/// Resource type tags (GPosition, GNormal, ...).
pub mod resource;
/// Transient resource table.
pub mod resource_table;
/// Pass scheduler and execution.
pub mod scheduler;
