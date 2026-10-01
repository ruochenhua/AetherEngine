//! Versioned Prefab assets, override application, and runtime instantiation.

mod asset;
mod document;
mod instantiate;
mod io;
mod migration;
mod overrides;
mod schema;
mod validation;

#[cfg(test)]
#[path = "prefab/tests.rs"]
mod tests;

pub use asset::PrefabAsset;
pub use instantiate::{instantiate, PrefabAssets, PrefabContext, ResolvedPrefabMaterial};
pub use schema::{
    ComponentPatch, PrefabComponentKind, PrefabDocument, PrefabError, PrefabInstanceConfig,
    PrefabNode, PrefabOverrides, RemovedComponent, PREFAB_SCHEMA_VERSION,
};
