//! Entity-Component-System (ECS) module.
//!
//! Thin wrapper around `hecs` providing engine-specific conveniences.
//!
//! ## Design
//!
//! - **Components** are plain data structs that implement `hecs::Component`.
//! - **Systems** are functions that operate on queries of components.
//! - **World** is the container for all entities and components.
//!
//! ## Example
//!
//! ```rust
//! use aether_engine::ecs::World;
//!
//! #[derive(Debug, Clone)]
//! struct Position { x: f32, y: f32 }
//!
//! impl hecs::Component for Position {}
//!
//! let mut world = World::new();
//! let entity = world.spawn((Position { x: 0.0, y: 0.0 },));
//! ```

pub use hecs::{Entity, Query, QueryBorrow, QueryMut, With, Without};

mod system;
mod world;

pub use system::{System, SystemRegistry};
pub use world::World;
