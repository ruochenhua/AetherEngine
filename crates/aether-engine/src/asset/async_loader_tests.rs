use super::*;
use crate::asset::Asset;
use std::fs;
use std::io::Write;
use std::thread;
use std::time::Duration;

#[derive(Debug, PartialEq)]
struct TestAsset(String);

impl Asset for TestAsset {
    fn load(path: &Path) -> anyhow::Result<Self> {
        Ok(TestAsset(fs::read_to_string(path)?))
    }
}

impl super::super::store::TypedAsset for TestAsset {
    const KIND: super::super::store::AssetKind = super::super::store::AssetKind::CpuMesh;
}

fn temp_path(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "aether_async_loader_{}_{}",
        std::process::id(),
        name
    ));
    path
}

fn wait_for_ready<T: TypedAsset + 'static + Send + Sync>(
    loader: &mut AsyncAssetLoader,
    handle: AsyncHandle<T>,
) -> std::sync::Arc<T> {
    for _ in 0..200 {
        loader.update();
        if let AssetLoadState::Ready(asset) = loader.state(handle) {
            return asset;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!("asset did not become ready in time");
}

#[test]
fn async_load_completes_with_asset_content() {
    let path = temp_path("completes");
    let mut file = fs::File::create(&path).unwrap();
    file.write_all(b"hello terrain").unwrap();

    let mut loader = AsyncAssetLoader::new();
    let handle = loader.load::<TestAsset>(&path);
    assert_eq!(
        wait_for_ready(&mut loader, handle).as_ref(),
        &TestAsset("hello terrain".into())
    );
    fs::remove_file(&path).ok();
}

#[test]
fn async_load_reports_failure_for_missing_file() {
    let path = temp_path("missing");
    let mut loader = AsyncAssetLoader::new();
    let handle = loader.load::<TestAsset>(&path);
    for _ in 0..200 {
        loader.update();
        if matches!(loader.state(handle), AssetLoadState::Failed(_)) {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(loader.state(handle), AssetLoadState::Failed(_)));
}

#[test]
fn async_load_state_is_loading_before_update() {
    let path = temp_path("loading_state");
    fs::write(&path, b"x").unwrap();
    let mut loader = AsyncAssetLoader::new();
    let handle = loader.load::<TestAsset>(&path);
    assert_eq!(loader.state(handle), AssetLoadState::Loading);
    fs::remove_file(&path).ok();
}

#[test]
fn default_loader_resolves_relative_paths_from_the_working_directory() {
    let file_name = format!("aether_async_relative_{}.txt", std::process::id());
    fs::write(&file_name, b"relative asset").unwrap();
    let mut loader = AsyncAssetLoader::new();
    let handle = loader.load::<TestAsset>(&file_name);

    assert_eq!(wait_for_ready(&mut loader, handle).0, "relative asset");
    fs::remove_file(file_name).unwrap();
}

#[test]
fn async_loader_adapter_deduplicates_repeated_kind_and_path_requests() {
    let path = temp_path("deduplicated");
    fs::write(&path, b"same asset").unwrap();
    let mut loader = AsyncAssetLoader::new();
    let first = loader.load::<TestAsset>(&path);
    let second = loader.load::<TestAsset>(&path);
    assert_eq!(first.id(), second.id());
    assert_eq!(wait_for_ready(&mut loader, first).0, "same asset");
    fs::remove_file(&path).ok();
}

#[test]
fn async_load_multiple_assets_complete() {
    let path_a = temp_path("multi_a");
    let path_b = temp_path("multi_b");
    fs::write(&path_a, b"asset_a").unwrap();
    fs::write(&path_b, b"asset_b").unwrap();
    let mut loader = AsyncAssetLoader::new();
    let handle_a = loader.load::<TestAsset>(&path_a);
    let handle_b = loader.load::<TestAsset>(&path_b);
    assert_eq!(wait_for_ready(&mut loader, handle_a).0, "asset_a");
    assert_eq!(wait_for_ready(&mut loader, handle_b).0, "asset_b");
    fs::remove_file(&path_a).ok();
    fs::remove_file(&path_b).ok();
}

#[test]
fn async_load_hot_reload_detects_mtime_change() {
    let path = temp_path("hot_reload");
    fs::write(&path, b"v1").unwrap();
    let mut loader = AsyncAssetLoader::new();
    let handle = loader.load::<TestAsset>(&path);
    let _ = wait_for_ready(&mut loader, handle);
    thread::sleep(Duration::from_millis(50));
    fs::write(&path, b"v2").unwrap();
    loader.update();
    assert_eq!(loader.state(handle), AssetLoadState::Loading);
    fs::remove_file(&path).ok();
}
