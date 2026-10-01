//! Versioned file watching and frame-boundary application for linked materials.

use aether_engine::asset::{
    material_asset::MaterialAsset, AssetManager, FrameBoundary, LoadStateView,
};
use aether_engine::ecs::components::{MaterialAssetRef, MaterialAssetStatus};
use aether_engine::ecs::{Entity, World};
use aether_engine::renderer::renderable::MaterialUniform;
use aether_engine::renderer::transparent::TransparentMaterial;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::SystemTime;

#[derive(Default)]
pub(crate) struct MaterialAssetWatcher {
    modified: HashMap<String, SystemTime>,
    pending: HashSet<String>,
    boundary: u64,
}

impl MaterialAssetWatcher {
    pub(crate) fn update(&mut self, world: &mut World, assets: &mut AssetManager) {
        self.boundary = self.boundary.saturating_add(1);
        self.apply_finished_reloads(world, assets);

        let paths = world
            .query::<&MaterialAssetRef>()
            .iter()
            .map(|reference| reference.0.clone())
            .collect::<HashSet<_>>();
        for path in paths {
            let modified = assets
                .project_root()
                .join(&path)
                .metadata()
                .and_then(|metadata| metadata.modified());
            let Ok(modified) = modified else {
                continue;
            };
            let Some(previous) = self.modified.get(&path).copied() else {
                self.modified.insert(path, modified);
                continue;
            };
            if previous == modified || self.pending.contains(&path) {
                continue;
            }
            self.modified.insert(path.clone(), modified);

            match assets
                .load::<MaterialAsset>(&path)
                .map_err(|error| error.to_string())
                .and_then(|handle| assets.reload(handle).map_err(|error| error.to_string()))
            {
                Ok(ticket) => {
                    self.pending.insert(path.clone());
                    tracing::info!(asset = %ticket.asset, generation = ticket.to_generation, "queued material asset reload");
                }
                Err(error) => {
                    tracing::error!(asset = %path, %error, "could not queue material asset reload");
                    set_material_status(world, &path, Some(error));
                }
            }
        }
    }

    fn apply_finished_reloads(&mut self, world: &mut World, assets: &mut AssetManager) {
        let results = match assets.poll_results(64) {
            Ok(results) => results,
            Err(error) => {
                tracing::error!(%error, "could not poll material asset reloads");
                return;
            }
        };
        for result in results {
            let path = result.request().asset.path().as_str().to_owned();
            match assets.apply_material_result(result, FrameBoundary::new(self.boundary)) {
                Ok(outcome) => match outcome.state {
                    LoadStateView::Ready { .. } => {
                        self.pending.remove(&path);
                        if let Err(error) = apply_material_generation(world, assets, &path) {
                            tracing::error!(asset = %path, %error, "material asset applied but scene consumers could not be updated");
                            set_material_status(world, &path, Some(error));
                        } else {
                            set_material_status(world, &path, None);
                            tracing::info!(asset = %path, generation = outcome.ticket.to_generation, "material asset reload committed");
                        }
                    }
                    LoadStateView::Failed { error, .. } => {
                        self.pending.remove(&path);
                        let message = error.to_string();
                        tracing::error!(asset = %path, %message, "material asset reload failed; keeping last good generation");
                        set_material_status(world, &path, Some(message));
                    }
                    LoadStateView::Loading { .. } => {
                        self.pending.remove(&path);
                    }
                },
                Err(error) => {
                    self.pending.remove(&path);
                    tracing::error!(asset = %path, %error, "material asset reload result was rejected");
                    set_material_status(world, &path, Some(error.to_string()));
                }
            }
        }
    }
}

fn apply_material_generation(
    world: &mut World,
    assets: &mut AssetManager,
    path: &str,
) -> Result<(), String> {
    let handle = assets
        .load::<MaterialAsset>(path)
        .map_err(|error| error.to_string())?;
    let asset = assets
        .get(handle)
        .ok_or_else(|| format!("material asset is unavailable: {path}"))?;
    let config = asset.config().clone();
    let resolution = asset
        .resolution()
        .ok_or_else(|| format!("material dependencies are unresolved: {path}"))?;
    let uniform = MaterialUniform::from_resolution(resolution);
    let transparent = config
        .transparent
        .as_ref()
        .map(|transparent| TransparentMaterial {
            base_color: config.albedo,
            texture: resolution.material.albedo,
            blend: transparent.blend,
            alpha_cutoff: transparent.alpha_cutoff,
        });
    let entities = {
        let mut query = world.query::<(Entity, &MaterialAssetRef)>();
        query
            .iter()
            .filter_map(|(entity, reference)| (reference.0 == path).then_some(entity))
            .collect::<Vec<_>>()
    };
    for entity in entities {
        world
            .insert(entity, (config.clone(), uniform))
            .map_err(|error| error.to_string())?;
        if let Some(material) = &transparent {
            world
                .insert(entity, (material.clone(),))
                .map_err(|error| error.to_string())?;
        } else {
            let _ = world.remove::<(TransparentMaterial,)>(entity);
        }
    }
    Ok(())
}

fn set_material_status(world: &mut World, path: &str, error: Option<String>) {
    let entities = {
        let mut query = world.query::<(Entity, &MaterialAssetRef)>();
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
            tracing::warn!(asset = %Path::new(path).display(), ?entity, "could not attach material reload status");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_engine::scene::MaterialConfig;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(1);

    struct Fixture(std::path::PathBuf);

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "aether_launcher_material_reload_{}_{}",
                std::process::id(),
                NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(root.join("materials")).unwrap();
            Self(root)
        }

        fn write(&self, text: &str) {
            fs::write(self.0.join("materials/test.ron"), text).unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn file_watch_commits_valid_generation_and_preserves_old_material_on_failure() {
        let fixture = Fixture::new();
        let original = MaterialConfig::default();
        fixture.write("(albedo: (0.8, 0.8, 0.8, 1.0), roughness: 0.5, metallic: 0.0)");
        let mut assets = AssetManager::with_project_root(&fixture.0);
        let handle = assets.load::<MaterialAsset>("materials/test.ron").unwrap();
        let asset_id = assets.asset_id(handle).unwrap();
        let root = assets.project_root().to_path_buf();
        let mut material = assets.get(handle).unwrap().as_ref().clone();
        material.resolve(&asset_id, &root, &mut assets).unwrap();
        let uniform = MaterialUniform::from_resolution(material.resolution().unwrap());
        let mut world = World::new();
        let entity = world.spawn((
            MaterialAssetRef("materials/test.ron".into()),
            MaterialAssetStatus::default(),
            original,
            uniform,
        ));
        let mut watcher = MaterialAssetWatcher::default();
        watcher.update(&mut world, &mut assets);

        let updated = MaterialConfig {
            albedo: [0.15, 0.35, 0.75, 1.0],
            ..MaterialConfig::default()
        };
        std::thread::sleep(Duration::from_millis(20));
        fixture.write("(albedo: (0.15, 0.35, 0.75, 1.0), roughness: 0.5, metallic: 0.0)");
        wait_for(&mut watcher, &mut world, &mut assets, |world| {
            world
                .query_one::<&MaterialUniform>(entity)
                .get()
                .is_ok_and(|uniform| uniform.albedo == updated.albedo)
        });
        assert_eq!(
            world.query_one::<&MaterialConfig>(entity).get().unwrap(),
            &updated
        );

        std::thread::sleep(Duration::from_millis(20));
        fixture.write("not a material document");
        wait_for(&mut watcher, &mut world, &mut assets, |world| {
            world
                .query_one::<&MaterialAssetStatus>(entity)
                .get()
                .is_ok_and(|status| status.0.is_some())
        });
        assert_eq!(
            world
                .query_one::<&MaterialUniform>(entity)
                .get()
                .unwrap()
                .albedo,
            updated.albedo
        );
        assert!(world
            .query_one::<&MaterialAssetStatus>(entity)
            .get()
            .unwrap()
            .0
            .is_some());
        assets.shutdown().unwrap();
    }

    fn wait_for(
        watcher: &mut MaterialAssetWatcher,
        world: &mut World,
        assets: &mut AssetManager,
        condition: impl Fn(&World) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            watcher.update(world, assets);
            if condition(world) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            condition(world),
            "timed out waiting for material file reload"
        );
    }
}
