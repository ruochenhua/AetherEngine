//! Entity-Component-System (ECS) module.
//!
//! Thin wrapper around `hecs` providing engine-specific conveniences.
//!
//! ## Design
//!
//! - **Components** are plain data structs with the `#[derive(Component)]` attribute.
//! - **Systems** are functions that operate on queries of components.
//! - **World** is the container for all entities and components.
//!
//! ## Example
//!
//! ```rust
//! use aether_engine::ecs::{World, Component};
//!
//! #[derive(Component)]
//! struct Position { x: f32, y: f32 }
//!
//! let mut world = World::new();
//! let entity = world.spawn((Position { x: 0.0, y: 0.0 },));
//! ```

pub use hecs::{Component, Entity, Query, QueryBorrow, QueryMut, With, Without};

mod system;
mod world;

pub use system::{System, SystemRegistry};
pub use world::World;
