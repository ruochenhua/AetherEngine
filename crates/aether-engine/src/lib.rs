//! Aether Engine
//!
//! A modern rendering engine built with Rust and wgpu.
//!
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
/// Deterministic simulation time control and transactional dispatch.
pub mod time;
/// Strict VisualCase v2 manifest schema and validation.
pub mod visual_case;
