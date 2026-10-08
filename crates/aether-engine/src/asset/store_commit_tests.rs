use super::store::{
    AssetError, AssetKind, AssetStore, AssetStoreConfig, FrameBoundary, LoadStateView, TypedAsset,
};
use super::Asset;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[derive(Debug)]
struct SlowText;

impl Asset for SlowText {
    fn load(path: &Path) -> anyhow::Result<Self> {
        thread::sleep(Duration::from_millis(30));
        fs::read_to_string(path)?;
        Ok(Self)
    }
}

impl TypedAsset for SlowText {
    const KIND: AssetKind = AssetKind::CpuMesh;
}

#[test]
fn cancelled_superseded_result_does_not_fail_the_newer_request() {
    let root =
        std::env::temp_dir().join(format!("aether_cancel_superseded_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("asset.txt"), "asset").unwrap();
    let mut store = AssetStore::new(AssetStoreConfig::new(root.clone()));
    let (handle, original_ticket) = store.request::<SlowText>(Path::new("asset.txt")).unwrap();
    store.cancel(&original_ticket).unwrap();
    store.reload(handle.clone()).unwrap();

    let old_result = loop {
        let mut results = store.poll_results(1).unwrap();
        if let Some(result) = results.pop() {
            break result;
        }
        thread::sleep(Duration::from_millis(2));
    };
    assert!(matches!(
        store.apply_result(old_result, FrameBoundary { frame_id: 1 }),
        Err(AssetError::Cancelled | AssetError::StaleReloadTicket)
    ));

    assert!(matches!(
        store.state(handle).unwrap(),
        LoadStateView::Loading { .. }
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn initial_result_stays_loading_until_frame_commit_callback_succeeds() {
    let root = std::env::temp_dir().join(format!("aether_staged_commit_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("asset.txt"), "asset").unwrap();
    let mut store = AssetStore::new(AssetStoreConfig::new(root.clone()));
    let (handle, _) = store.request::<SlowText>(Path::new("asset.txt")).unwrap();
    let result = loop {
        if let Some(result) = store.poll_results(1).unwrap().pop() {
            break result;
        }
        thread::sleep(Duration::from_millis(2));
    };
    let ticket = store
        .apply_result(result, FrameBoundary { frame_id: 1 })
        .unwrap();
    assert!(matches!(
        store.state(handle.clone()),
        Ok(LoadStateView::Loading { generation: 1 })
    ));
    assert!(store.get(handle.clone()).is_err());

    let committed = AtomicUsize::new(0);
    store
        .commit_result(
            &ticket,
            FrameBoundary { frame_id: 1 },
            |asset, generation, payload| {
                assert_eq!(asset.kind, AssetKind::CpuMesh);
                assert_eq!(generation, 1);
                assert!(payload.is::<SlowText>());
                committed.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(committed.load(Ordering::SeqCst), 1);
    assert!(matches!(
        store.state(handle),
        Ok(LoadStateView::Ready { generation: 1 })
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn frame_commit_rejects_non_owner_thread_before_running_callback() {
    let root =
        std::env::temp_dir().join(format!("aether_wrong_commit_thread_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("asset.txt"), "asset").unwrap();
    let mut store = AssetStore::new(AssetStoreConfig::new(root.clone()));
    let (handle, _) = store.request::<SlowText>(Path::new("asset.txt")).unwrap();
    let result = loop {
        if let Some(result) = store.poll_results(1).unwrap().pop() {
            break result;
        }
        thread::sleep(Duration::from_millis(2));
    };
    let ticket = store
        .apply_result(result, FrameBoundary { frame_id: 1 })
        .unwrap();
    let callback_count = Arc::new(AtomicUsize::new(0));
    let callback_count_for_thread = callback_count.clone();
    let (store, result) = thread::spawn(move || {
        let result = store.commit_result(&ticket, FrameBoundary { frame_id: 1 }, |_, _, _| {
            callback_count_for_thread.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        (store, result)
    })
    .join()
    .unwrap();
    assert_eq!(result, Err(AssetError::WrongCommitThread));
    assert_eq!(callback_count.load(Ordering::SeqCst), 0);
    assert!(matches!(
        store.state(handle),
        Ok(LoadStateView::Loading { generation: 1 })
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn staged_result_cannot_commit_after_cancellation() {
    let root = std::env::temp_dir().join(format!("aether_cancel_staged_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("asset.txt"), "asset").unwrap();
    let mut store = AssetStore::new(AssetStoreConfig::new(root.clone()));
    let (handle, request) = store.request::<SlowText>(Path::new("asset.txt")).unwrap();
    let result = loop {
        if let Some(result) = store.poll_results(1).unwrap().pop() {
            break result;
        }
        thread::sleep(Duration::from_millis(2));
    };
    let accepted = store
        .apply_result(result, FrameBoundary { frame_id: 1 })
        .unwrap();
    store.cancel(&request).unwrap();
    assert_eq!(
        store.commit_result(&accepted, FrameBoundary { frame_id: 1 }, |_, _, _| Ok(())),
        Err(AssetError::Cancelled)
    );
    assert!(matches!(
        store.state(handle),
        Ok(LoadStateView::Failed {
            error: AssetError::Cancelled,
            ..
        })
    ));
    fs::remove_dir_all(root).unwrap();
}
