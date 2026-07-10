//! Aether Engine
//!
//! A modern rendering engine built with Rust and wgpu.
//!
//! ## Architecture
//!
//! - **ECS**: Entity-Component-System architecture using `hecs`
//! - **RenderGraph**: Declarative render pass scheduling with automatic resource management
//! - **Modular Systems**: Renderer, Physics, Audio, Animation as independent systems

#![warn(missing_docs)]

pub mod asset;
pub mod clouds;
pub mod ecs;
/// Input state manager.
pub mod input;
pub mod math;
pub mod physics;
pub mod renderer;
pub mod scene;
pub mod terrain;
