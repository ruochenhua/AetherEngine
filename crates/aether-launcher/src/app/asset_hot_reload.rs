//! Prefab and texture file watching with frame-boundary generation commits.

use aether_engine::asset::registry::BuiltinMeshRegistry;
use aether_engine::asset::{
    material_asset::MaterialAsset, prefab::PrefabAsset, texture::CpuTexture, Asset, AssetId,
    AssetKind, AssetManager, FrameBoundary, GpuCommitCache, LoadStateView,
};
use aether_engine::ecs::components::{
    MaterialAssetRef, MaterialAssetStatus, PrefabAssetStatus, PrefabInstanceRoot,
};
use aether_engine::ecs::World;
use aether_engine::scene::loader::SceneLoader;
use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

#[cfg(test)]
#[path = "asset_hot_reload_tests.rs"]
mod tests;

#[derive(Default)]
pub(crate) struct AssetHotReloadWatcher {
    prefab_modified: HashMap<String, SystemTime>,
    pending_prefabs: HashSet<String>,
    texture_modified: HashMap<String, SystemTime>,
    pending_textures: HashSet<String>,
    boundary: u64,
}

impl AssetHotReloadWatcher {
    pub(crate) fn update(
        &mut self,
        world: &mut World,
        assets: &mut AssetManager,
        texture_cache: &mut dyn GpuCommitCache,
        device: &wgpu::Device,
        registry: &BuiltinMeshRegistry,
    ) {
        self.boundary = self.boundary.saturating_add(1);
        self.apply_finished_reloads(world, assets, texture_cache, device, registry);
        self.watch_prefab_files(world, assets);
        self.watch_texture_files(world, assets);
    }

    fn apply_finished_reloads(
        &mut self,
        world: &mut World,
        assets: &mut AssetManager,
        texture_cache: &mut dyn GpuCommitCache,
        device: &wgpu::Device,
        registry: &BuiltinMeshRegistry,
    ) {
        let results = match assets.poll_results(128) {
            Ok(results) => results,
            Err(error) => {
                tracing::error!(%error, "could not poll asset hot reload results");
                return;
            }
        };
        for result in results {
            let asset = result.request().asset.clone();
            let path = asset.path().as_str().to_owned();
            if asset.kind() == AssetKind::Material {
                assets.defer_result(result);
                continue;
            }
            if !matches!(asset.kind(), AssetKind::Prefab | AssetKind::CpuTexture) {
                match assets.apply_result(result, FrameBoundary::new(self.boundary)) {
                    Ok(outcome) if matches!(outcome.state, LoadStateView::Ready { .. }) => {}
                    Ok(_) => {}
                    Err(error) => {
                        tracing::error!(asset = %path, %error, "unhandled asset load result was rejected")
                    }
                }
                continue;
            }

            let texture_dependents = if asset.kind() == AssetKind::CpuTexture {
                dependent_materials(world, assets, &asset)
            } else {
                Vec::new()
            };
            match assets.apply_result(result, FrameBoundary::new(self.boundary)) {
                Ok(outcome) => match outcome.state {
                    LoadStateView::Ready { .. } if asset.kind() == AssetKind::Prefab => {
                        self.pending_prefabs.remove(&path);
                        match SceneLoader::reload_prefab_instances(
                            &path, device, registry, assets, world,
                        ) {
                            Ok(count) => {
                                tracing::info!(asset = %path, instances = count, generation = outcome.ticket.to_generation, "prefab generation committed to scene instances")
                            }
                            Err(error) => {
                                let message = error.to_string();
                                set_prefab_status(world, &path, Some(message.clone()));
                                tracing::error!(asset = %path, %message, "prefab generation could not be applied; current instances retained")
                            }
                        }
                    }
                    LoadStateView::Ready { .. } => {
                        self.pending_textures.remove(&path);
                        match assets.commit_gpu(
                            outcome.ticket.clone(),
                            texture_cache,
                            FrameBoundary::new(self.boundary),
                        ) {
                            Ok(()) => {
                                for material in texture_dependents {
                                    match queue_asset_reload::<MaterialAsset>(assets, &material) {
                                        Ok(()) => {
                                            tracing::info!(asset = %material, texture = %path, "queued dependent material refresh")
                                        }
                                        Err(error) => {
                                            tracing::error!(asset = %material, %error, "could not refresh material after texture reload");
                                            set_material_status(
                                                world,
                                                &material,
                                                Some(error.to_string()),
                                            );
                                        }
                                    }
                                }
                                tracing::info!(asset = %path, generation = outcome.ticket.to_generation, "texture GPU generation committed at frame boundary");
                            }
                            Err(error) => {
                                let message = error.to_string();
                                tracing::error!(asset = %path, %message, "texture GPU generation commit failed");
                                for material in texture_dependents {
                                    set_material_status(world, &material, Some(message.clone()));
                                }
                            }
                        }
                    }
                    LoadStateView::Failed { error, .. } => {
                        self.clear_pending(&asset.kind(), &path);
                        let message = error.to_string();
                        tracing::error!(asset = %path, %message, "asset reload failed; preserving last-known-good generation");
                        if asset.kind() == AssetKind::Prefab {
                            set_prefab_status(world, &path, Some(message.clone()));
                        }
                        for material in texture_dependents {
                            set_material_status(world, &material, Some(message.clone()));
                        }
                    }
                    LoadStateView::Loading { .. } => self.clear_pending(&asset.kind(), &path),
                },
                Err(error) => {
                    self.clear_pending(&asset.kind(), &path);
                    tracing::error!(asset = %path, %error, "asset reload result was rejected");
                    if asset.kind() == AssetKind::Prefab {
                        set_prefab_status(world, &path, Some(error.to_string()));
                    }
                    for material in texture_dependents {
                        set_material_status(world, &material, Some(error.to_string()));
                    }
                }
            }
        }
    }

    fn clear_pending(&mut self, kind: &AssetKind, path: &str) {
        match kind {
            AssetKind::Prefab => {
                self.pending_prefabs.remove(path);
            }
            AssetKind::CpuTexture => {
                self.pending_textures.remove(path);
            }
            _ => {}
        }
    }

    fn watch_prefab_files(&mut self, world: &World, assets: &mut AssetManager) {
        let paths = world
            .query::<&PrefabInstanceRoot>()
            .iter()
            .map(|root| root.0.prefab_asset.clone())
            .collect::<HashSet<_>>();
        for path in paths {
            let modified = assets
                .project_root()
                .join(&path)
                .metadata()
                .and_then(|metadata| metadata.modified());
            let Ok(modified) = modified else { continue };
            let Some(previous) = self.prefab_modified.get(&path).copied() else {
                self.prefab_modified.insert(path, modified);
                continue;
            };
            if previous == modified || self.pending_prefabs.contains(&path) {
                continue;
            }
            match queue_asset_reload::<PrefabAsset>(assets, &path) {
                Ok(()) => {
                    self.prefab_modified.insert(path.clone(), modified);
                    self.pending_prefabs.insert(path.clone());
                    tracing::info!(asset = %path, "queued prefab hot reload");
                }
                Err(error) => {
                    tracing::error!(asset = %path, %error, "could not queue prefab hot reload")
                }
            }
        }
    }

    fn watch_texture_files(&mut self, world: &mut World, assets: &mut AssetManager) {
        let material_paths = world
            .query::<&MaterialAssetRef>()
            .iter()
            .map(|reference| reference.0.clone())
            .collect::<HashSet<_>>();
        let mut dependencies = HashMap::<String, (HashSet<String>, bool)>::new();
        for material_path in material_paths {
            let Ok(handle) = assets.load::<MaterialAsset>(&material_path) else {
                continue;
            };
            let Some(material) = assets.get(handle) else {
                continue;
            };
            for dependency in material.dependencies() {
                let entry = dependencies
                    .entry(dependency.asset.path().as_str().to_owned())
                    .or_default();
                entry.0.insert(material_path.clone());
                entry.1 |= matches!(
                    dependency.status,
                    aether_engine::asset::material_asset::MaterialDependencyStatus::MissingFallback(
                        _
                    )
                );
            }
        }
        for (path, (materials, was_missing)) in dependencies {
            let modified = assets
                .project_root()
                .join(&path)
                .metadata()
                .and_then(|metadata| metadata.modified());
            let Ok(modified) = modified else { continue };
            let previous = self.texture_modified.get(&path).copied();
            let changed = previous.is_some_and(|previous| previous != modified)
                || (previous.is_none() && was_missing);
            if !changed || self.pending_textures.contains(&path) {
                self.texture_modified.entry(path).or_insert(modified);
                continue;
            }
            match queue_asset_reload::<CpuTexture>(assets, &path) {
                Ok(()) => {
                    self.texture_modified.insert(path.clone(), modified);
                    self.pending_textures.insert(path.clone());
                    tracing::info!(asset = %path, consumers = materials.len(), "queued texture hot reload");
                }
                Err(error) => {
                    tracing::error!(asset = %path, %error, "could not queue texture hot reload");
                    for material in materials {
                        set_material_status(world, &material, Some(error.to_string()));
                    }
                }
            }
        }
    }
}

fn queue_asset_reload<T: Asset>(
    assets: &mut AssetManager,
    path: &str,
) -> Result<(), aether_engine::asset::AssetError> {
    match assets.load::<T>(path) {
        Ok(handle) => assets.reload(handle).map(|_| ()),
        Err(_) => assets.request::<T>(path).map(|_| ()),
    }
}

fn dependent_materials(world: &World, assets: &mut AssetManager, asset: &AssetId) -> Vec<String> {
    let paths = world
        .query::<&MaterialAssetRef>()
        .iter()
        .map(|reference| reference.0.clone())
        .collect::<HashSet<_>>();
    paths
        .into_iter()
        .filter(|path| {
            assets
                .load::<MaterialAsset>(path)
                .ok()
                .and_then(|handle| assets.get(handle))
                .is_some_and(|material| {
                    material
                        .dependencies()
                        .iter()
                        .any(|dependency| &dependency.asset == asset)
                })
        })
        .collect()
}

fn set_material_status(world: &mut World, path: &str, error: Option<String>) {
    let entities = {
        let mut query = world.query::<(aether_engine::ecs::Entity, &MaterialAssetRef)>();
        query
            .iter()
            .filter_map(|(entity, reference)| (reference.0 == path).then_some(entity))
            .collect::<Vec<_>>()
    };
    for entity in entities {
        if world
            .insert(entity, (MaterialAssetStatus(error.clone()),))
            .is_err()
        {
            tracing::warn!(asset = %path, ?entity, "could not attach asset reload status");
        }
    }
}

fn set_prefab_status(world: &mut World, path: &str, error: Option<String>) {
    let entities = {
        let mut query = world.query::<(aether_engine::ecs::Entity, &PrefabInstanceRoot)>();
        query
            .iter()
            .filter_map(|(entity, root)| (root.0.prefab_asset == path).then_some(entity))
            .collect::<Vec<_>>()
    };
    for entity in entities {
        if world
            .insert(entity, (PrefabAssetStatus(error.clone()),))
            .is_err()
        {
            tracing::warn!(asset = %path, ?entity, "could not attach Prefab reload status");
        }
    }
}
