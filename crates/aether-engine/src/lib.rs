//! Aether Engine
//! 
//! A modern rendering engine built with Rust and wgpu.
//! 
//! ## Architecture
//! 
//! - **ECS**: Entity-Component-System architecture using `hecs`
//! - **RenderGraph**: Declarative render pass scheduling with automatic resource management
//! - **Modular Systems**: Renderer, Physics, Audio, Animation as independent systems
//! 
//! ## Usage
//! 
//! ```rust,no_run
//! use aether_engine::App;
//!
//! fn main() {
//!     let mut app = App::new("Aether", 1280, 720);
//!     app.run();
//! }
//! ```

#![warn(missing_docs)]

pub mod app;
pub mod asset;
pub mod ecs;
pub mod input;
pub mod math;
pub mod physics;
pub mod renderer;
pub mod scene;
pub mod window;

pub use app::App;
