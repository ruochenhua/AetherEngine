//! Built-in mesh registry.
//!
//! Maps string names to `CpuMesh` factory functions so scene files can
//! reference built-in shapes by name.

use crate::asset::mesh::CpuMesh;
use std::collections::HashMap;

/// Registry of built-in mesh factories, indexed by name.
pub struct BuiltinMeshRegistry {
    factories: HashMap<String, fn() -> CpuMesh>,
}

impl BuiltinMeshRegistry {
    /// Create a new registry pre-populated with common shapes.
    pub fn new() -> Self {
        let mut registry = Self {
            factories: HashMap::new(),
        };
        registry.register("cube", || CpuMesh::cube());
        registry.register("sphere", || CpuMesh::sphere(32));
        registry.register("quad", || CpuMesh::quad());
        registry.register("plane", || CpuMesh::plane());
        registry
    }

    /// Register a new built-in mesh under the given name.
    ///
    /// Overwrites any existing entry with the same name.
    pub fn register(&mut self, name: &str, factory: fn() -> CpuMesh) {
        self.factories.insert(name.to_string(), factory);
    }

    /// Look up a built-in mesh by name.
    ///
    /// Returns `None` if the name is not registered.
    pub fn get(&self, name: &str) -> Option<CpuMesh> {
        self.factories.get(name).map(|f| f())
    }

    /// List all registered mesh names.
    pub fn names(&self) -> Vec<&str> {
        self.factories.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for BuiltinMeshRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_registered_meshes_exist() {
        let registry = BuiltinMeshRegistry::new();
        assert!(registry.get("cube").is_some());
        assert!(registry.get("sphere").is_some());
        assert!(registry.get("quad").is_some());
    }

    #[test]
    fn unknown_name_returns_none() {
        let registry = BuiltinMeshRegistry::new();
        assert!(registry.get("dragon").is_none());
    }

    #[test]
    fn register_new_mesh() {
        let mut registry = BuiltinMeshRegistry::new();
        // Register a custom mesh (just re-use cube factory for testing)
        registry.register("custom", || CpuMesh::cube());
        assert!(registry.get("custom").is_some());
    }

    #[test]
    fn overwrite_existing_mesh() {
        let mut registry = BuiltinMeshRegistry::new();
        let old = registry.get("cube").unwrap();
        registry.register("cube", || CpuMesh::sphere(16));
        let new = registry.get("cube").unwrap();
        // After overwrite, get returns the new mesh (sphere, not cube)
        assert_ne!(old.positions.len(), new.positions.len());
    }

    #[test]
    fn names_returns_all_registered() {
        let registry = BuiltinMeshRegistry::new();
        let mut names = registry.names();
        names.sort();
        assert!(names.contains(&"cube"));
        assert!(names.contains(&"sphere"));
        assert!(names.contains(&"quad"));
        assert!(names.contains(&"plane"));
        assert_eq!(names.len(), 4);
    }
}
