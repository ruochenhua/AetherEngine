//! Render passes.
//!
//! Each pass is a self-contained rendering stage.
//! Passes are registered with the RenderGraph and executed in dependency order.

pub mod debug;
pub mod gbuffer;
pub mod lighting;
pub mod shadow;
pub mod template;
