use super::World;
use tracing::trace;

/// A system that operates on the ECS World.
///
/// Systems are the primary unit of logic in the engine.
/// Each system should have a single, well-defined responsibility.
pub trait System {
    /// System name for debugging and profiling.
    fn name(&self) -> &str;

    /// Initialize the system.
    fn init(&mut self, _world: &mut World) {}

    /// Update the system.
    ///
    /// Called once per frame (or at the system's configured frequency).
    fn update(&mut self, dt: f32, world: &mut World);

    /// Shutdown the system.
    fn shutdown(&mut self, _world: &mut World) {}
}

/// Registry of all systems.
///
/// Manages system execution order and lifecycle.
pub struct SystemRegistry {
    startup_systems: Vec<Box<dyn System>>,
    update_systems: Vec<Box<dyn System>>,
    fixed_update_systems: Vec<Box<dyn System>>,
    shutdown_systems: Vec<Box<dyn System>>,
}

impl Default for SystemRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            startup_systems: Vec::new(),
            update_systems: Vec::new(),
            fixed_update_systems: Vec::new(),
            shutdown_systems: Vec::new(),
        }
    }

    /// Register a startup system.
    pub fn add_startup(&mut self, system: impl System + 'static) {
        trace!("Registering startup system: {}", system.name());
        self.startup_systems.push(Box::new(system));
    }

    /// Register an update system.
    pub fn add_update(&mut self, system: impl System + 'static) {
        trace!("Registering update system: {}", system.name());
        self.update_systems.push(Box::new(system));
    }

    /// Register a fixed update system.
    pub fn add_fixed_update(&mut self, system: impl System + 'static) {
        trace!("Registering fixed update system: {}", system.name());
        self.fixed_update_systems.push(Box::new(system));
    }

    /// Register a shutdown system.
    pub fn add_shutdown(&mut self, system: impl System + 'static) {
        trace!("Registering shutdown system: {}", system.name());
        self.shutdown_systems.push(Box::new(system));
    }

    /// Run all startup systems.
    pub fn run_startup(&mut self, world: &mut World) {
        for system in &mut self.startup_systems {
            trace!("Running startup: {}", system.name());
            system.init(world);
        }
    }

    /// Run all update systems.
    pub fn run_update(&mut self, dt: f32, world: &mut World) {
        for system in &mut self.update_systems {
            system.update(dt, world);
        }
    }

    /// Run all fixed update systems.
    pub fn run_fixed_update(&mut self, dt: f32, world: &mut World) {
        for system in &mut self.fixed_update_systems {
            system.update(dt, world);
        }
    }

    /// Run all shutdown systems.
    pub fn run_shutdown(&mut self, world: &mut World) {
        for system in &mut self.shutdown_systems {
            trace!("Running shutdown: {}", system.name());
            system.shutdown(world);
        }
    }
}
