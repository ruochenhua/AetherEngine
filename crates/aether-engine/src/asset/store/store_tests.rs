use super::*;
use crate::asset::{Asset, AssetId, AssetKind};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug, Eq, PartialEq)]
pub(super) struct TextAsset(String);

impl Asset for TextAsset {
    const KIND: AssetKind = AssetKind::GltfDocument;

    fn load(path: &Path) -> anyhow::Result<Self> {
        let value = fs::read_to_string(path)?;
        if value == "fail" {
            anyhow::bail!("fixture requested a decode failure");
        }
        if value.starts_with("slow:") {
            thread::sleep(Duration::from_millis(40));
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Eq, PartialEq)]
struct OtherTextAsset(String);

impl Asset for OtherTextAsset {
    const KIND: AssetKind = AssetKind::Material;

    fn load(path: &Path) -> anyhow::Result<Self> {
        Ok(Self(fs::read_to_string(path)?))
    }
}

#[derive(Debug, Eq, PartialEq)]
struct SameKindOtherAsset;

impl Asset for SameKindOtherAsset {
    const KIND: AssetKind = AssetKind::GltfDocument;

    fn load(_path: &Path) -> anyhow::Result<Self> {
        Ok(Self)
    }
}

struct WrongKindPayload;

impl AssetPayload for WrongKindPayload {
    fn kind(&self) -> AssetKind {
        AssetKind::Material
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn into_any(self: Arc<Self>) -> Arc<dyn std::any::Any + Send + Sync> {
        self
    }
}

pub(super) struct FixtureDir(PathBuf);

impl FixtureDir {
    pub(super) fn new() -> Self {
        let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("aether_asset_store_{}_{}", std::process::id(), id));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub(super) fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, contents).unwrap();
        path
    }

    pub(super) fn store(&self) -> AssetStore {
        AssetStore::new(AssetStoreConfig::new(&self.0))
    }
}

impl Drop for FixtureDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn finish_one<T: Asset>(store: &mut AssetStore, handle: Handle<T>) -> ApplyOutcome {
    for frame in 1..=500 {
        let mut target_outcome = None;
        for result in store.poll_results(8).unwrap() {
            let is_target = result.request().slot == handle.slot();
            let outcome = store
                .apply_result(result, FrameBoundary::new(frame))
                .unwrap();
            if is_target {
                target_outcome = Some(outcome);
            }
        }
        if let Some(outcome) = target_outcome {
            return outcome;
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("asset handle {:?} did not finish", store.state(handle));
}

#[test]
fn same_path_with_different_kinds_has_distinct_persistent_ids_and_slots() {
    let fixture = FixtureDir::new();
    fixture.write("shared.asset", "shared data");
    let mut store = fixture.store();
    let (mesh, _) = store
        .request::<TextAsset>(Path::new("shared.asset"))
        .unwrap();
    let (material, _) = store
        .request::<OtherTextAsset>(Path::new("shared.asset"))
        .unwrap();

    assert_ne!(mesh.slot(), material.slot());
    assert_eq!(mesh.generation(), 1);
    assert_eq!(material.generation(), 1);
    assert_ne!(
        AssetId::from_path(
            AssetKind::GltfDocument,
            &fixture.0,
            Path::new("shared.asset")
        )
        .unwrap(),
        AssetId::from_path(AssetKind::Material, &fixture.0, Path::new("shared.asset")).unwrap()
    );
    for frame in 1..=500 {
        for result in store.poll_results(8).unwrap() {
            store
                .apply_result(result, FrameBoundary::new(frame))
                .unwrap();
        }
        let mesh_ready = matches!(
            store.state(mesh),
            Ok(LoadStateView::Ready { generation: 1 })
        );
        let material_ready = matches!(
            store.state(material),
            Ok(LoadStateView::Ready { generation: 1 })
        );
        if mesh_ready && material_ready {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(store.get(mesh).unwrap().0, "shared data");
    assert_eq!(store.get(material).unwrap().0, "shared data");
    store.shutdown().unwrap();
}

#[test]
fn async_load_exposes_loading_then_owned_typed_payload() {
    let fixture = FixtureDir::new();
    fixture.write("model.gltf", "owned document");
    let mut store = fixture.store();
    let (handle, _) = store.request::<TextAsset>(Path::new("model.gltf")).unwrap();
    assert_eq!(
        store.state(handle).unwrap(),
        LoadStateView::Loading { generation: 1 }
    );
    assert_eq!(store.get(handle).unwrap_err(), AssetError::NotReady);

    finish_one(&mut store, handle);
    let loaded = store.get(handle).unwrap();
    assert_eq!(loaded.as_ref(), &TextAsset("owned document".into()));
    assert_eq!(
        store.state(handle).unwrap(),
        LoadStateView::Ready { generation: 1 }
    );
    store.shutdown().unwrap();
}

#[test]
fn duplicate_in_flight_request_reuses_handle_and_cancellation_capability() {
    let fixture = FixtureDir::new();
    fixture.write("model.gltf", "slow:in flight");
    let mut store = fixture.store();
    let first = store.request::<TextAsset>(Path::new("model.gltf")).unwrap();
    let second = store
        .request::<TextAsset>(Path::new("models/../model.gltf"))
        .unwrap();

    assert_eq!(first.0.slot(), second.0.slot());
    assert_eq!(first.1, second.1);
    finish_one(&mut store, first.0);
    store.shutdown().unwrap();
}

#[test]
fn reload_success_increments_generation_and_invalidates_old_handle() {
    let fixture = FixtureDir::new();
    fixture.write("model.gltf", "version one");
    let mut store = fixture.store();
    let (old_handle, _) = store.request::<TextAsset>(Path::new("model.gltf")).unwrap();
    finish_one(&mut store, old_handle);

    fixture.write("model.gltf", "version two");
    let ticket = store.reload(old_handle).unwrap();
    assert_eq!(ticket.from_generation, 1);
    assert_eq!(ticket.to_generation, 2);
    assert_eq!(store.get(old_handle).unwrap().0, "version one");
    for _ in 0..500 {
        let mut completed = None;
        for result in store.poll_results(8).unwrap() {
            // `finish_one` may apply the initial load on any frame up to 500.
            completed = Some(store.apply_result(result, FrameBoundary::new(501)).unwrap());
        }
        if completed.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }

    assert_eq!(store.get(old_handle).unwrap_err(), AssetError::StaleHandle);
    let asset_id = ticket.asset.clone();
    let new_handle = store.current_handle::<TextAsset>(&asset_id).unwrap();
    assert_eq!(new_handle.generation(), 2);
    assert_eq!(store.get(new_handle).unwrap().0, "version two");
    store.shutdown().unwrap();
}

#[test]
fn failed_reload_keeps_the_previous_generation_available() {
    let fixture = FixtureDir::new();
    fixture.write("model.gltf", "last known good");
    let mut store = fixture.store();
    let (handle, _) = store.request::<TextAsset>(Path::new("model.gltf")).unwrap();
    finish_one(&mut store, handle);

    fixture.write("model.gltf", "fail");
    let _ticket = store.reload(handle).unwrap();
    let outcome = finish_one(&mut store, handle);
    assert!(matches!(
        outcome.state,
        LoadStateView::Failed {
            has_last_good: true,
            ..
        }
    ));
    assert_eq!(store.get(handle).unwrap().0, "last known good");
    store.shutdown().unwrap();
}

#[test]
fn cancellation_is_idempotent_and_cannot_publish_a_late_payload() {
    let fixture = FixtureDir::new();
    fixture.write("model.gltf", "slow:cancel me");
    let mut store = fixture.store();
    let (handle, ticket) = store.request::<TextAsset>(Path::new("model.gltf")).unwrap();
    store.cancel(&ticket).unwrap();
    store.cancel(&ticket).unwrap();
    assert_eq!(
        store.state(handle).unwrap(),
        LoadStateView::Failed {
            generation: 1,
            error: AssetError::Cancelled,
            has_last_good: false,
        }
    );
    thread::sleep(Duration::from_millis(60));
    for result in store.poll_results(8).unwrap() {
        assert_eq!(
            store.apply_result(result, FrameBoundary::new(1)),
            Err(AssetError::Cancelled)
        );
    }
    assert_eq!(store.get(handle).unwrap_err(), AssetError::Cancelled);
    store.shutdown().unwrap();
}

#[test]
fn reload_ticket_commits_at_most_once_on_the_calling_thread() {
    #[derive(Default)]
    struct FakeCache {
        commits: usize,
        caller: Option<thread::ThreadId>,
    }
    impl GpuCommitCache for FakeCache {
        fn commit_asset(
            &mut self,
            _key: GpuAssetKey,
            _payload: Arc<dyn AssetPayload>,
            _boundary: FrameBoundary,
        ) -> Result<(), AssetError> {
            self.commits += 1;
            self.caller = Some(thread::current().id());
            Ok(())
        }
        fn collect_retired(&mut self) {}
    }

    let fixture = FixtureDir::new();
    fixture.write("model.gltf", "asset");
    let app_thread = thread::current().id();
    let mut store = fixture.store();
    let (handle, _) = store.request::<TextAsset>(Path::new("model.gltf")).unwrap();
    let outcome = finish_one(&mut store, handle);
    let ticket = outcome.ticket;
    let mut cache = FakeCache::default();
    {
        let mut context = GpuCommitContext {
            cache: &mut cache,
            boundary: FrameBoundary::new(2),
        };
        store.commit_gpu(ticket.clone(), &mut context).unwrap();
        store.commit_gpu(ticket, &mut context).unwrap();
    }

    assert_eq!(cache.commits, 1);
    assert_eq!(cache.caller, Some(app_thread));
    store.shutdown().unwrap();
}

#[test]
fn shutdown_timeout_is_retryable_and_requests_fail_after_shutdown_starts() {
    let fixture = FixtureDir::new();
    fixture.write("slow.gltf", "slow:shutdown");
    let mut config = AssetStoreConfig::new(&fixture.0);
    config.join_timeout_ms = 0;
    let mut store = AssetStore::new(config);
    let _ = store.request::<TextAsset>(Path::new("slow.gltf")).unwrap();
    thread::sleep(Duration::from_millis(10));
    assert_eq!(store.shutdown(), Err(AssetError::ShutdownTimeout));
    assert_eq!(
        store.request::<TextAsset>(Path::new("next.gltf")),
        Err(AssetError::WorkerStopped)
    );
    thread::sleep(Duration::from_millis(60));
    store.shutdown().unwrap();
    store.shutdown().unwrap();
}

#[test]
fn stale_handles_and_unknown_tickets_fail_closed() {
    let fixture = FixtureDir::new();
    fixture.write("model.gltf", "asset");
    let mut store = fixture.store();
    let (handle, ticket) = store.request::<TextAsset>(Path::new("model.gltf")).unwrap();
    let mut forged = ticket;
    forged.id = u128::MAX;
    assert_eq!(store.cancel(&forged), Err(AssetError::StaleHandle));
    finish_one(&mut store, handle);
    let stale = Handle::<TextAsset>::from_parts(handle.slot(), handle.generation() + 1);
    assert_eq!(store.state(stale), Err(AssetError::StaleHandle));
    store.shutdown().unwrap();
}

#[test]
fn polling_is_bounded_and_applies_each_worker_result_once() {
    let fixture = FixtureDir::new();
    fixture.write("first.gltf", "first");
    fixture.write("second.gltf", "second");
    let mut store = fixture.store();
    store.request::<TextAsset>(Path::new("first.gltf")).unwrap();
    store
        .request::<TextAsset>(Path::new("second.gltf"))
        .unwrap();

    let mut applied = 0;
    for frame in 1..=500 {
        let results = store.poll_results(1).unwrap();
        assert!(results.len() <= 1);
        for result in results {
            store
                .apply_result(result, FrameBoundary::new(frame))
                .unwrap();
            applied += 1;
        }
        if applied == 2 {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(applied, 2);
    store.shutdown().unwrap();
}

#[test]
fn late_reload_result_cannot_replace_a_newer_requested_generation() {
    let fixture = FixtureDir::new();
    fixture.write("model.gltf", "version one");
    let mut store = fixture.store();
    let (handle, _) = store.request::<TextAsset>(Path::new("model.gltf")).unwrap();
    finish_one(&mut store, handle);

    fixture.write("model.gltf", "version two");
    let stale_ticket = store.reload(handle).unwrap();
    let stale_result = loop {
        let mut ready = None;
        for result in store.poll_results(1).unwrap() {
            ready = Some(result);
        }
        if let Some(result) = ready {
            break result;
        }
        thread::sleep(Duration::from_millis(2));
    };

    fixture.write("model.gltf", "version three");
    let current_ticket = store.reload(handle).unwrap();
    assert_eq!(stale_ticket.to_generation, 2);
    assert_eq!(current_ticket.to_generation, 3);
    assert_eq!(
        store.apply_result(stale_result, FrameBoundary::new(2)),
        Err(AssetError::Cancelled)
    );
    assert_eq!(store.get(handle).unwrap().0, "version one");

    for frame in 3..=500 {
        if let Some(result) = store.poll_results(1).unwrap().into_iter().next() {
            store
                .apply_result(result, FrameBoundary::new(frame))
                .unwrap();
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    let current = store
        .current_handle::<TextAsset>(&current_ticket.asset)
        .unwrap();
    assert_eq!(current.generation(), 3);
    assert_eq!(store.get(current).unwrap().0, "version three");
    store.shutdown().unwrap();
}

#[test]
fn payload_kind_and_runtime_type_mismatches_fail_closed() {
    let fixture = FixtureDir::new();
    fixture.write("kind.gltf", "valid");
    fixture.write("type.gltf", "valid");
    let mut store = fixture.store();

    let (kind_handle, _) = store.request::<TextAsset>(Path::new("kind.gltf")).unwrap();
    let kind_request = store.entries[&kind_handle.slot()]
        .pending
        .as_ref()
        .unwrap()
        .request
        .clone();
    assert_eq!(
        store.apply_result(
            AssetResult::Ready {
                request: kind_request,
                payload: Arc::new(WrongKindPayload),
            },
            FrameBoundary::new(1),
        ),
        Err(AssetError::PayloadKindMismatch)
    );
    assert!(matches!(
        store.state(kind_handle),
        Ok(LoadStateView::Failed {
            error: AssetError::PayloadKindMismatch,
            ..
        })
    ));

    let (type_handle, _) = store.request::<TextAsset>(Path::new("type.gltf")).unwrap();
    let type_request = store.entries[&type_handle.slot()]
        .pending
        .as_ref()
        .unwrap()
        .request
        .clone();
    assert_eq!(
        store.apply_result(
            AssetResult::Ready {
                request: type_request,
                payload: Arc::new(SameKindOtherAsset),
            },
            FrameBoundary::new(1),
        ),
        Err(AssetError::PayloadTypeMismatch)
    );
    assert!(matches!(
        store.state(type_handle),
        Ok(LoadStateView::Failed {
            error: AssetError::PayloadTypeMismatch,
            ..
        })
    ));
    store.shutdown().unwrap();
}
