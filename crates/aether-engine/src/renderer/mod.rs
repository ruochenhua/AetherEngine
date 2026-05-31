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
/// Per-frame data context (RenderFrame).
pub mod frame;
/// Light component definitions.
pub mod light;
/// Pass trait and type-safe resource handles.
pub mod pass;
/// Shared renderable types (Renderable, MaterialUniform, ...).
pub mod renderable;
/// Render pass implementations.
pub mod passes;
/// Resource type tags (GPosition, GNormal, ...).
pub mod resource;
/// Transient resource table.
pub mod resource_table;
/// Pipeline builder and scheduler.
pub mod scheduler;
