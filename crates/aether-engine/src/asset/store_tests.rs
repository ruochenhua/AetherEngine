use super::store::{
    canonicalize, downcast_owned, AssetError, AssetId, AssetKind, AssetPayload, AssetStore,
    AssetStoreConfig, FrameBoundary, LoadStateView, TypedAsset,
};
use super::{Asset, Handle};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[derive(Debug, PartialEq)]
struct TextAsset(String);

impl Asset for TextAsset {
    fn load(path: &Path) -> anyhow::Result<Self> {
        Ok(Self(fs::read_to_string(path)?))
    }
}

impl TypedAsset for TextAsset {
    const KIND: AssetKind = AssetKind::CpuMesh;
}

static COUNTING_LOADS: AtomicUsize = AtomicUsize::new(0);
static CANCELLABLE_LOAD_STARTED: AtomicUsize = AtomicUsize::new(0);
static CANCELLABLE_LOAD_CANCELLED: AtomicUsize = AtomicUsize::new(0);

struct CountingAsset;

impl Asset for CountingAsset {
    fn load(path: &Path) -> anyhow::Result<Self> {
        COUNTING_LOADS.fetch_add(1, Ordering::SeqCst);
        if fs::read_to_string(path)? == "slow" {
            thread::sleep(Duration::from_millis(80));
        }
        Ok(Self)
    }
}

impl TypedAsset for CountingAsset {
    const KIND: AssetKind = AssetKind::CpuMesh;
}

struct OtherMeshAsset;

impl Asset for OtherMeshAsset {
    fn load(_path: &Path) -> anyhow::Result<Self> {
        Ok(Self)
    }
}

impl TypedAsset for OtherMeshAsset {
    const KIND: AssetKind = AssetKind::CpuMesh;
}

struct CooperativeAsset;

impl Asset for CooperativeAsset {
    fn load(_path: &Path) -> anyhow::Result<Self> {
        Ok(Self)
    }
}

impl TypedAsset for CooperativeAsset {
    const KIND: AssetKind = AssetKind::CpuMesh;

    fn load_cancellable(
        _path: &Path,
        cancellation: &super::store::CancellationToken,
    ) -> Result<Self, AssetError> {
        CANCELLABLE_LOAD_STARTED.fetch_add(1, Ordering::SeqCst);
        loop {
            if cancellation.is_cancelled() {
                CANCELLABLE_LOAD_CANCELLED.fetch_add(1, Ordering::SeqCst);
                return Err(AssetError::Cancelled);
            }
            thread::sleep(Duration::from_millis(1));
        }
    }
}

fn root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("aether_store_{}_{}", std::process::id(), name));
    fs::create_dir_all(&root).expect("create temporary asset root");
    root
}

fn wait_for_result<T: TypedAsset>(store: &mut AssetStore, handle: &Handle<T>) {
    for _ in 0..200 {
        let results = store.poll_results(8).expect("poll results");
        for result in results {
            store
                .apply_result(result, FrameBoundary { frame_id: 1 })
                .expect("apply result");
        }
        if matches!(store.state(handle.clone()), Ok(LoadStateView::Ready { .. })) {
            return;
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("asset did not load in time");
}

#[test]
fn canonicalizer_rejects_paths_that_escape_project_root() {
    let root = root("escape");
    assert!(matches!(
        canonicalize(&root, Path::new("../../secret.png")),
        Err(AssetError::PathEscape)
    ));
    assert!(matches!(
        canonicalize(&root, Path::new("/tmp/secret.png")),
        Err(AssetError::PathEscape)
    ));
    assert_eq!(
        canonicalize(&root, Path::new("textures/../albedo.png"))
            .unwrap()
            .as_str(),
        "albedo.png"
    );
}

#[test]
fn asset_id_uses_stable_kind_path_string_serialization() {
    let id = AssetId {
        kind: AssetKind::CpuMesh,
        path: canonicalize(Path::new("/project"), Path::new("models/cube.obj")).unwrap(),
    };
    let encoded = serde_json::to_string(&id).unwrap();
    assert_eq!(encoded, "\"CpuMesh:models/cube.obj\"");
    assert_eq!(serde_json::from_str::<AssetId>(&encoded).unwrap(), id);
}

#[test]
fn request_rejects_a_second_rust_type_for_the_same_persistent_asset_id() {
    let root = root("typed_identity");
    fs::write(root.join("mesh.txt"), "mesh").unwrap();
    let mut store = AssetStore::new(AssetStoreConfig::new(root));
    store.request::<TextAsset>(Path::new("mesh.txt")).unwrap();
    assert!(matches!(
        store.request::<OtherMeshAsset>(Path::new("mesh.txt")),
        Err(AssetError::PayloadTypeMismatch)
    ));
}

#[test]
fn owned_payload_downcast_checks_kind_and_concrete_type() {
    let payload: Arc<dyn AssetPayload> = Arc::new(TextAsset("mesh".into()));
    assert_eq!(downcast_owned::<TextAsset>(payload).unwrap().0, "mesh");

    let wrong_kind: Arc<dyn AssetPayload> =
        Arc::new(super::texture::CpuTexture::from_color(0, 0, 0, 255));
    assert!(matches!(
        downcast_owned::<TextAsset>(wrong_kind),
        Err(AssetError::PayloadKindMismatch)
    ));

    let wrong_type: Arc<dyn AssetPayload> = Arc::new(OtherMeshAsset);
    assert!(matches!(
        downcast_owned::<TextAsset>(wrong_type),
        Err(AssetError::PayloadTypeMismatch)
    ));
}

#[test]
fn requests_deduplicate_by_kind_and_canonical_path() {
    let root = root("dedupe");
    fs::write(root.join("mesh.txt"), "mesh").unwrap();
    let mut store = AssetStore::new(AssetStoreConfig::new(root.clone()));
    let (first, _) = store
        .request::<TextAsset>(Path::new("folder/../mesh.txt"))
        .unwrap();
    let (second, _) = store.request::<TextAsset>(Path::new("mesh.txt")).unwrap();
    assert_eq!(first, second);
    wait_for_result(&mut store, &first);
    assert_eq!(store.get(first.clone()).unwrap().0, "mesh");
}

#[test]
fn cancellation_is_idempotent_and_prevents_late_commit() {
    let root = root("cancel");
    fs::write(root.join("mesh.txt"), "mesh").unwrap();
    let mut store = AssetStore::new(AssetStoreConfig::new(root));
    let (handle, ticket) = store.request::<TextAsset>(Path::new("mesh.txt")).unwrap();
    let copied_ticket = ticket;
    store.cancel(&ticket).unwrap();
    store.cancel(&copied_ticket).unwrap();
    for _ in 0..100 {
        let results = store.poll_results(8).unwrap();
        for result in results {
            assert!(matches!(
                store.apply_result(result, FrameBoundary { frame_id: 1 }),
                Err(AssetError::Cancelled)
            ));
        }
        if matches!(
            store.state(handle.clone()),
            Ok(LoadStateView::Failed { .. })
        ) {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(store.get(handle).is_err());
}

#[test]
fn stale_handles_return_an_error_after_release() {
    let root = root("stale");
    fs::write(root.join("mesh.txt"), "mesh").unwrap();
    let mut store = AssetStore::new(AssetStoreConfig::new(root));
    let (handle, _) = store.request::<TextAsset>(Path::new("mesh.txt")).unwrap();
    wait_for_result(&mut store, &handle);
    store.release(handle.clone()).unwrap();
    assert!(matches!(store.state(handle), Err(AssetError::StaleHandle)));
}

#[test]
fn shutdown_is_repeatable_and_stops_new_requests() {
    let root = root("shutdown");
    let mut store = AssetStore::new(AssetStoreConfig::new(root));
    store.shutdown().unwrap();
    store.shutdown().unwrap();
    assert!(matches!(
        store.request::<TextAsset>(Path::new("mesh.txt")),
        Err(AssetError::WorkerStopped)
    ));
}

#[test]
fn polling_is_bounded_and_results_share_a_frame_boundary() {
    let root = root("bounded");
    fs::write(root.join("one.txt"), "one").unwrap();
    fs::write(root.join("two.txt"), "two").unwrap();
    let mut store = AssetStore::new(AssetStoreConfig::new(root));
    let (one, _) = store.request::<TextAsset>(Path::new("one.txt")).unwrap();
    let (two, _) = store.request::<TextAsset>(Path::new("two.txt")).unwrap();
    let mut applied = 0;
    for _ in 0..200 {
        let results = store.poll_results(1).unwrap();
        assert!(results.len() <= 1);
        for result in results {
            store
                .apply_result(result, FrameBoundary { frame_id: 7 })
                .unwrap();
            applied += 1;
        }
        if applied == 2 {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(applied, 2);
    assert_eq!(store.get(one).unwrap().0, "one");
    assert_eq!(store.get(two).unwrap().0, "two");
}

#[test]
fn failed_reload_keeps_last_good_payload() {
    let root = root("last_good");
    fs::write(root.join("mesh.txt"), "initial").unwrap();
    let mut store = AssetStore::new(AssetStoreConfig::new(root.clone()));
    let (handle, _) = store.request::<TextAsset>(Path::new("mesh.txt")).unwrap();
    wait_for_result(&mut store, &handle);
    fs::remove_file(root.join("mesh.txt")).unwrap();
    store.reload(handle.clone()).unwrap();

    let mut failed = false;
    for _ in 0..200 {
        for result in store.poll_results(1).unwrap() {
            assert!(store
                .apply_result(result, FrameBoundary { frame_id: 2 })
                .is_err());
            failed = true;
        }
        if failed {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(matches!(
        store.state(handle.clone()),
        Ok(LoadStateView::Failed {
            has_last_good: true,
            ..
        })
    ));
    assert_eq!(store.get(handle).unwrap().0, "initial");
}

#[test]
fn successful_reload_advances_generation_for_subsequent_requests() {
    let root = root("generation");
    fs::write(root.join("mesh.txt"), "initial").unwrap();
    let mut store = AssetStore::new(AssetStoreConfig::new(root.clone()));
    let (old_handle, _) = store.request::<TextAsset>(Path::new("mesh.txt")).unwrap();
    wait_for_result(&mut store, &old_handle);
    fs::write(root.join("mesh.txt"), "updated").unwrap();
    store.reload(old_handle.clone()).unwrap();
    for _ in 0..200 {
        let mut loaded = false;
        for result in store.poll_results(1).unwrap() {
            store
                .apply_result(result, FrameBoundary { frame_id: 2 })
                .unwrap();
            loaded = true;
        }
        if loaded {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(matches!(
        store.state(old_handle.clone()),
        Err(AssetError::StaleHandle)
    ));
    let (new_handle, _) = store.request::<TextAsset>(Path::new("mesh.txt")).unwrap();
    assert_eq!(new_handle.generation(), old_handle.generation() + 1);
    assert_eq!(store.get(new_handle).unwrap().0, "updated");
}

#[test]
fn cancelling_queued_work_prevents_the_worker_from_decoding_it() {
    COUNTING_LOADS.store(0, Ordering::SeqCst);
    let root = root("queued_cancel");
    fs::write(root.join("slow.txt"), "slow").unwrap();
    fs::write(root.join("cancel.txt"), "cancel").unwrap();
    let mut store = AssetStore::new(AssetStoreConfig::new(root));
    let (slow, _) = store
        .request::<CountingAsset>(Path::new("slow.txt"))
        .unwrap();
    let (cancelled, ticket) = store
        .request::<CountingAsset>(Path::new("cancel.txt"))
        .unwrap();
    store.cancel(&ticket).unwrap();

    let mut outcomes = Vec::new();
    for _ in 0..200 {
        for result in store.poll_results(2).unwrap() {
            outcomes.push(store.apply_result(result, FrameBoundary { frame_id: 1 }));
        }
        if outcomes.len() == 2 {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(COUNTING_LOADS.load(Ordering::SeqCst), 1);
    assert!(matches!(
        store.state(slow).unwrap(),
        LoadStateView::Ready { .. }
    ));
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    assert!(outcomes
        .iter()
        .any(|result| matches!(result, Err(AssetError::Cancelled))));
    assert!(matches!(
        store.state(cancelled),
        Ok(LoadStateView::Failed { .. })
    ));
}

#[test]
fn cancellation_token_stops_a_cooperative_decode_already_in_progress() {
    CANCELLABLE_LOAD_STARTED.store(0, Ordering::SeqCst);
    CANCELLABLE_LOAD_CANCELLED.store(0, Ordering::SeqCst);
    let root = root("active_cancel");
    fs::write(root.join("active.txt"), "active").unwrap();
    let mut store = AssetStore::new(AssetStoreConfig::new(root));
    let (handle, ticket) = store
        .request::<CooperativeAsset>(Path::new("active.txt"))
        .unwrap();

    for _ in 0..200 {
        if CANCELLABLE_LOAD_STARTED.load(Ordering::SeqCst) == 1 {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(CANCELLABLE_LOAD_STARTED.load(Ordering::SeqCst), 1);
    store.cancel(&ticket).unwrap();

    for _ in 0..200 {
        if CANCELLABLE_LOAD_CANCELLED.load(Ordering::SeqCst) == 1 {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(CANCELLABLE_LOAD_CANCELLED.load(Ordering::SeqCst), 1);
    let result = store.poll_results(1).unwrap().pop().unwrap();
    assert!(matches!(
        store.apply_result(result, FrameBoundary { frame_id: 1 }),
        Err(AssetError::Cancelled)
    ));
    assert!(store.get(handle).is_err());
}

#[test]
fn shutdown_cancels_queued_decodes_and_is_retryable_after_timeout() {
    COUNTING_LOADS.store(0, Ordering::SeqCst);
    let root = root("shutdown_cancel");
    fs::write(root.join("slow.txt"), "slow").unwrap();
    fs::write(root.join("queued.txt"), "queued").unwrap();
    let mut config = AssetStoreConfig::new(root);
    config.join_timeout = Duration::from_millis(1);
    let mut store = AssetStore::new(config);
    store
        .request::<CountingAsset>(Path::new("slow.txt"))
        .unwrap();
    store
        .request::<CountingAsset>(Path::new("queued.txt"))
        .unwrap();
    for _ in 0..50 {
        if COUNTING_LOADS.load(Ordering::SeqCst) == 1 {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }

    assert!(matches!(store.shutdown(), Err(AssetError::ShutdownTimeout)));
    thread::sleep(Duration::from_millis(100));
    store.shutdown().unwrap();
    assert_eq!(COUNTING_LOADS.load(Ordering::SeqCst), 1);
    assert!(matches!(
        store.request::<CountingAsset>(Path::new("queued.txt")),
        Err(AssetError::WorkerStopped)
    ));
}

#[test]
fn out_of_order_reload_results_cannot_replace_the_newer_payload() {
    let root = root("out_of_order");
    fs::write(root.join("mesh.txt"), "initial").unwrap();
    let mut store = AssetStore::new(AssetStoreConfig::new(root.clone()));
    let (handle, _) = store.request::<TextAsset>(Path::new("mesh.txt")).unwrap();
    wait_for_result(&mut store, &handle);
    fs::write(root.join("mesh.txt"), "older queued reload").unwrap();
    store.reload(handle.clone()).unwrap();
    fs::write(root.join("mesh.txt"), "newer reload").unwrap();
    store.reload(handle).unwrap();

    let mut outcomes = Vec::new();
    for _ in 0..200 {
        for result in store.poll_results(2).unwrap() {
            outcomes.push(store.apply_result(result, FrameBoundary { frame_id: 2 }));
        }
        if outcomes.len() == 2 {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(matches!(&outcomes[0], Err(AssetError::StaleReloadTicket)));
    assert!(outcomes[1].is_ok());
    let (current, _) = store.request::<TextAsset>(Path::new("mesh.txt")).unwrap();
    assert_eq!(store.get(current).unwrap().0, "newer reload");
}
