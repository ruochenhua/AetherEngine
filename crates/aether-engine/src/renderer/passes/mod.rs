//! Render passes.
//!
//! Each pass is a self-contained rendering stage.
//! Passes are registered with the RenderGraph and executed in dependency order.

pub mod bloom;
pub mod composite;
pub mod debug;
pub mod fxaa;
pub mod gbuffer;
pub mod lighting;
pub mod shadow;
pub mod ssao;
pub mod ssr;
pub mod tone_mapping;
pub mod template;
