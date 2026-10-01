use super::super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[derive(Debug, Eq, PartialEq)]
struct HotAsset(String);

impl Asset for HotAsset {
    const KIND: crate::asset::AssetKind = crate::asset::AssetKind::CpuMesh;

    fn load(path: &Path) -> anyhow::Result<Self> {
        let value = fs::read_to_string(path)?;
        anyhow::ensure!(value != "fail", "fixture requested a decode failure");
        Ok(Self(value))
    }
}

fn wait_ready(loader: &mut AsyncAssetLoader, handle: AsyncHandle<HotAsset>) -> Arc<HotAsset> {
    for _ in 0..300 {
        loader.update();
        if let AssetLoadState::Ready(asset) = loader.state(handle) {
            return asset;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!("asset did not become ready in time");
}

#[test]
fn auto_reload_retries_after_failure_and_keeps_last_good_until_success() {
    let path: PathBuf = std::env::temp_dir().join(format!(
        "aether_async_hot_reload_retry_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(&path, "v1").unwrap();
    let mut loader = AsyncAssetLoader::with_project_root(path.parent().unwrap());
    let handle = loader.load::<HotAsset>(&path);
    assert_eq!(
        wait_ready(&mut loader, handle).as_ref(),
        &HotAsset("v1".into())
    );
    let first_generation = to_sync_handle(handle);

    thread::sleep(Duration::from_millis(80));
    fs::write(&path, "fail").unwrap();
    for _ in 0..300 {
        loader.update();
        if matches!(loader.state(handle), AssetLoadState::Failed(_)) {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(loader.state(handle), AssetLoadState::Failed(_)));
    assert_eq!(
        loader.store.get(first_generation).unwrap().as_ref(),
        &HotAsset("v1".into())
    );

    thread::sleep(Duration::from_millis(80));
    fs::write(&path, "v2").unwrap();
    assert_eq!(
        wait_ready(&mut loader, handle).as_ref(),
        &HotAsset("v2".into())
    );
    loader.shutdown().unwrap();
    fs::remove_file(path).ok();
}
