use super::{MaterialAsset, MaterialDependencyStatus};
use crate::asset::{Asset, AssetError, AssetId, AssetKind, AssetManager};
use crate::scene::config::MaterialConfig;
use crate::scene::material::{TextureFallback, TextureUsage};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_ROOT: AtomicUsize = AtomicUsize::new(1);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "aether_t8_material_asset_{}_{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, relative: &str, bytes: impl AsRef<[u8]>) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn material_id() -> AssetId {
    AssetId::from_path(
        AssetKind::Material,
        Path::new("."),
        Path::new("materials/paint.ron"),
    )
    .unwrap()
}

#[test]
fn material_asset_loads_ron_and_rejects_corrupt_documents() {
    let fixture = Fixture::new();
    let valid = ron::to_string(&MaterialConfig::default()).unwrap();
    fixture.write("paint.ron", valid);
    fixture.write("broken.ron", "not a material config");

    let loaded = MaterialAsset::load(&fixture.0.join("paint.ron")).unwrap();
    assert_eq!(loaded.config(), &MaterialConfig::default());
    assert!(MaterialAsset::load(&fixture.0.join("broken.ron")).is_err());
}

#[test]
fn missing_texture_keeps_t3_fallback_and_records_dependency() {
    let fixture = Fixture::new();
    let config = MaterialConfig {
        normal_texture: Some("textures/missing.png".into()),
        ..MaterialConfig::default()
    };
    let mut asset = MaterialAsset::from_config(config);
    let mut textures = AssetManager::with_project_root(&fixture.0);

    asset
        .resolve(&material_id(), &fixture.0, &mut textures)
        .unwrap();

    assert_eq!(asset.dependencies().len(), 1);
    assert_eq!(asset.dependencies()[0].usage, TextureUsage::Normal);
    assert_eq!(
        asset.dependencies()[0].status,
        MaterialDependencyStatus::MissingFallback(TextureFallback::FlatNormal)
    );
    assert!(asset.resolution().is_some());
}

#[test]
fn corrupt_texture_returns_a_typed_dependency_chain() {
    let fixture = Fixture::new();
    fixture.write("materials/textures/broken.png", b"not a png");
    let config = MaterialConfig {
        albedo_texture: Some("textures/broken.png".into()),
        ..MaterialConfig::default()
    };
    let mut asset = MaterialAsset::from_config(config);
    let mut textures = AssetManager::with_project_root(&fixture.0);

    let error = asset
        .resolve(&material_id(), &fixture.0, &mut textures)
        .unwrap_err();
    assert!(matches!(
        error,
        AssetError::Dependency { asset, cause }
            if asset.kind() == AssetKind::CpuTexture
                && asset.path().as_str() == "materials/textures/broken.png"
                && matches!(*cause, AssetError::Decode(_))
    ));
}

#[test]
fn texture_path_cannot_escape_project_root() {
    let fixture = Fixture::new();
    let config = MaterialConfig {
        albedo_texture: Some("../../../outside.png".into()),
        ..MaterialConfig::default()
    };
    let mut asset = MaterialAsset::from_config(config);
    let mut textures = AssetManager::with_project_root(&fixture.0);

    assert!(matches!(
        asset.resolve(&material_id(), &fixture.0, &mut textures),
        Err(AssetError::InvalidPath(_))
    ));
}
