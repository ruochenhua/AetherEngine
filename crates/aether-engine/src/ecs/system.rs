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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Records every lifecycle call into a shared log so tests can
    /// assert execution order across systems and stages.
    struct RecordingSystem {
        name: &'static str,
        log: Rc<RefCell<Vec<String>>>,
    }

    impl RecordingSystem {
        fn new(name: &'static str, log: Rc<RefCell<Vec<String>>>) -> Self {
            Self { name, log }
        }
    }

    impl System for RecordingSystem {
        fn name(&self) -> &str {
            self.name
        }

        fn init(&mut self, _world: &mut World) {
            self.log.borrow_mut().push(format!("init:{}", self.name));
        }

        fn update(&mut self, dt: f32, _world: &mut World) {
            self.log
                .borrow_mut()
                .push(format!("update:{}:{dt}", self.name));
        }

        fn shutdown(&mut self, _world: &mut World) {
            self.log
                .borrow_mut()
                .push(format!("shutdown:{}", self.name));
        }
    }

    #[test]
    fn run_startup_executes_systems_in_registration_order() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut registry = SystemRegistry::new();
        registry.add_startup(RecordingSystem::new("first", Rc::clone(&log)));
        registry.add_startup(RecordingSystem::new("second", Rc::clone(&log)));
        registry.add_startup(RecordingSystem::new("third", Rc::clone(&log)));

        let mut world = World::new();
        registry.run_startup(&mut world);

        assert_eq!(
            log.borrow().as_slice(),
            ["init:first", "init:second", "init:third"],
            "startup systems must be initialized in registration order"
        );
    }

    #[test]
    fn run_update_executes_systems_in_registration_order() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut registry = SystemRegistry::new();
        registry.add_update(RecordingSystem::new("first", Rc::clone(&log)));
        registry.add_update(RecordingSystem::new("second", Rc::clone(&log)));
        registry.add_update(RecordingSystem::new("third", Rc::clone(&log)));

        let mut world = World::new();
        registry.run_update(0.016, &mut world);

        assert_eq!(
            log.borrow().as_slice(),
            [
                "update:first:0.016",
                "update:second:0.016",
                "update:third:0.016"
            ],
            "update systems must run in registration order with the given dt"
        );
    }

    #[test]
    fn run_fixed_update_executes_systems_in_registration_order() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut registry = SystemRegistry::new();
        registry.add_fixed_update(RecordingSystem::new("first", Rc::clone(&log)));
        registry.add_fixed_update(RecordingSystem::new("second", Rc::clone(&log)));
        registry.add_fixed_update(RecordingSystem::new("third", Rc::clone(&log)));

        let mut world = World::new();
        registry.run_fixed_update(0.016, &mut world);

        assert_eq!(
            log.borrow().as_slice(),
            [
                "update:first:0.016",
                "update:second:0.016",
                "update:third:0.016"
            ],
            "fixed update systems must run in registration order with the given dt"
        );
    }

    #[test]
    fn run_shutdown_executes_systems_in_registration_order() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut registry = SystemRegistry::new();
        registry.add_shutdown(RecordingSystem::new("first", Rc::clone(&log)));
        registry.add_shutdown(RecordingSystem::new("second", Rc::clone(&log)));
        registry.add_shutdown(RecordingSystem::new("third", Rc::clone(&log)));

        let mut world = World::new();
        registry.run_shutdown(&mut world);

        assert_eq!(
            log.borrow().as_slice(),
            ["shutdown:first", "shutdown:second", "shutdown:third"],
            "shutdown systems must run in registration order"
        );
    }

    #[test]
    fn each_stage_runs_only_its_own_systems() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut registry = SystemRegistry::new();
        registry.add_startup(RecordingSystem::new("startup", Rc::clone(&log)));
        registry.add_update(RecordingSystem::new("update", Rc::clone(&log)));
        registry.add_fixed_update(RecordingSystem::new("fixed", Rc::clone(&log)));
        registry.add_shutdown(RecordingSystem::new("shutdown", Rc::clone(&log)));

        let mut world = World::new();
        registry.run_startup(&mut world);
        registry.run_update(0.5, &mut world);
        registry.run_fixed_update(0.25, &mut world);
        registry.run_shutdown(&mut world);

        assert_eq!(
            log.borrow().as_slice(),
            [
                "init:startup",
                "update:update:0.5",
                "update:fixed:0.25",
                "shutdown:shutdown"
            ],
            "each run_* method must execute only the systems registered for that stage, with that call's dt"
        );
    }
}
