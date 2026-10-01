use aether_engine::asset::{
    canonicalize, Asset, AssetError, AssetId, AssetKind, AssetPayload, AssetStore,
    AssetStoreConfig, FrameBoundary, GpuAssetKey, GpuCommitCache, GpuCommitContext, LoadStateView,
};
use aether_engine::visual_case::{ExpectedResult, ProbeValue, VisualManifest};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, ThreadId};
use std::time::Duration;

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug, Eq, PartialEq)]
struct TextAsset(String);

impl Asset for TextAsset {
    const KIND: AssetKind = AssetKind::CpuMesh;

    fn load(path: &Path) -> anyhow::Result<Self> {
        let text = fs::read_to_string(path)?;
        if text.starts_with("slow:") {
            thread::sleep(Duration::from_millis(40));
        }
        if text == "fail" {
            anyhow::bail!("fixture requested a decode failure");
        }
        Ok(Self(text))
    }
}

struct FixtureRoot(PathBuf);

impl FixtureRoot {
    fn new() -> Self {
        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "aether_t8_asset_store_{}_{}",
            std::process::id(),
            sequence
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, text: &str) {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, text).unwrap();
    }
}

impl Drop for FixtureRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct CommitRecorder {
    commits: usize,
    commit_thread: Option<ThreadId>,
    keys: Vec<GpuAssetKey>,
}

impl GpuCommitCache for CommitRecorder {
    fn commit_asset(
        &mut self,
        key: GpuAssetKey,
        _payload: Arc<dyn AssetPayload>,
        _boundary: FrameBoundary,
    ) -> Result<(), AssetError> {
        self.commits += 1;
        self.commit_thread = Some(thread::current().id());
        self.keys.push(key);
        Ok(())
    }

    fn collect_retired(&mut self) {}
}

fn wait_for_result<T: Asset>(
    store: &mut AssetStore,
    handle: aether_engine::asset::Handle<T>,
    first_frame: u64,
) -> (Result<aether_engine::asset::ApplyOutcome, AssetError>, u64) {
    for offset in 0..500 {
        let frame = first_frame + offset;
        for result in store.poll_results(8).unwrap() {
            let is_target = result.request().slot == handle.slot();
            let outcome = store.apply_result(result, FrameBoundary::new(frame));
            if is_target {
                return (outcome, frame);
            }
            outcome.unwrap();
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("AssetStore did not produce a result for the requested handle");
}

fn validate_visual_case(project_root: &Path) {
    let case_path = project_root.join("tests/cases/t8_asset_store_lifecycle.json");
    let source = fs::read_to_string(case_path).unwrap();
    let manifest = VisualManifest::from_json_in(&source, project_root).unwrap();
    let materialized = manifest.materialize().unwrap();
    assert_eq!(materialized.len(), 1);
    assert_eq!(materialized[0].id, "t8_asset_store_lifecycle__default");
    match materialized[0].expected_result.as_ref().unwrap() {
        ExpectedResult::Render { probes, .. } => {
            let names: Vec<_> = probes.iter().map(|probe| probe.name.as_str()).collect();
            assert_eq!(
                names,
                [
                    "initial_load_commit",
                    "cancel_idempotent",
                    "worker_gpu_commit",
                    "state_machine"
                ]
            );
            assert_eq!(probes[0].value, ProbeValue::Bool(true));
            assert_eq!(probes[1].value, ProbeValue::Bool(true));
            assert_eq!(probes[2].value, ProbeValue::Bool(false));
            assert_eq!(probes[3].value, ProbeValue::Text("valid".into()));
        }
        ExpectedResult::ExpectedError { .. } => panic!("primary fixture must render"),
    }
}

fn write_events(probes: Value, events: &[&str]) {
    let Some(path) = std::env::var_os("AETHER_T8_ASSET_EVENTS") else {
        return;
    };
    let path = PathBuf::from(path);
    fs::write(
        path,
        serde_json::to_vec_pretty(&json!({
            "case_id": "t8_asset_store_lifecycle",
            "kind": "Fixture",
            "events": events,
            "probes": probes
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn t8_asset_store_lifecycle() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap();
    validate_visual_case(workspace);

    let fixture = FixtureRoot::new();
    fixture.write("asset.txt", "last known good");
    fixture.write("cancel.txt", "slow:cancelled request");

    let canonical = canonicalize(&fixture.0, Path::new("nested/../asset.txt")).unwrap();
    assert_eq!(canonical.as_str(), "asset.txt");
    let mesh_id =
        AssetId::from_path(AssetKind::CpuMesh, &fixture.0, Path::new("asset.txt")).unwrap();
    let texture_id =
        AssetId::from_path(AssetKind::CpuTexture, &fixture.0, Path::new("asset.txt")).unwrap();
    assert_ne!(mesh_id, texture_id);
    let same_kind = AssetId::from_path(
        AssetKind::CpuMesh,
        &fixture.0,
        Path::new("nested/../asset.txt"),
    )
    .unwrap();
    assert_eq!(mesh_id, same_kind);

    let mut store = AssetStore::new(AssetStoreConfig::new(&fixture.0));
    let (handle, ticket) = store.request::<TextAsset>(Path::new("asset.txt")).unwrap();
    let copied_ticket = ticket;
    assert_eq!(ticket, copied_ticket);
    let (duplicate_handle, duplicate_ticket) = store
        .request::<TextAsset>(Path::new("nested/../asset.txt"))
        .unwrap();
    assert_eq!(handle, duplicate_handle);
    assert_eq!(ticket, duplicate_ticket);
    assert_eq!(
        store.state(handle).unwrap(),
        LoadStateView::Loading { generation: 1 }
    );

    let (initial, initial_frame) = wait_for_result(&mut store, handle, 1);
    let initial = initial.unwrap();
    assert_eq!(initial.ticket.from_generation, 0);
    assert_eq!(initial.ticket.to_generation, 1);
    assert_eq!(store.get(handle).unwrap().0, "last known good");
    assert_eq!(
        store.state(handle).unwrap(),
        LoadStateView::Ready { generation: 1 }
    );

    let caller = thread::current().id();
    let mut recorder = CommitRecorder::default();
    let commit_frame = FrameBoundary::new(initial_frame + 1);
    let mut context = GpuCommitContext {
        cache: &mut recorder,
        boundary: commit_frame,
    };
    store
        .commit_gpu(initial.ticket.clone(), &mut context)
        .unwrap();
    store.commit_gpu(initial.ticket, &mut context).unwrap();
    let initial_load_committed = recorder.commits == 1
        && recorder.commit_thread == Some(caller)
        && recorder.keys
            == [GpuAssetKey {
                asset: mesh_id,
                generation: 1,
            }];
    assert!(initial_load_committed);
    let worker_gpu_commit = recorder.commit_thread.is_some_and(|id| id != caller);

    fixture.write("asset.txt", "fail");
    let reload = store.reload(handle).unwrap();
    assert_eq!(reload.to_generation, 2);
    assert_eq!(store.get(handle).unwrap().0, "last known good");
    let (failed_reload, failed_frame) = wait_for_result(&mut store, handle, initial_frame + 2);
    let failed_reload = failed_reload.unwrap();
    assert!(matches!(
        failed_reload.state,
        LoadStateView::Failed {
            generation: 2,
            has_last_good: true,
            ..
        }
    ));
    assert_eq!(store.get(handle).unwrap().0, "last known good");

    let (cancel_handle, cancel_ticket) =
        store.request::<TextAsset>(Path::new("cancel.txt")).unwrap();
    store.cancel(&cancel_ticket).unwrap();
    store.cancel(&cancel_ticket).unwrap();
    let mut late_result_rejected = false;
    for offset in 0..500 {
        let frame = failed_frame + 1 + offset;
        if let Some(result) = store.poll_results(1).unwrap().into_iter().next() {
            assert_eq!(result.request().slot, cancel_handle.slot());
            late_result_rejected =
                store.apply_result(result, FrameBoundary::new(frame)) == Err(AssetError::Cancelled);
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(late_result_rejected);
    assert_eq!(store.get(cancel_handle), Err(AssetError::Cancelled));
    let cancel_idempotent = store.state(cancel_handle).is_ok()
        && store.cancel(&cancel_ticket).is_ok()
        && late_result_rejected;
    assert!(cancel_idempotent);

    let state_machine = store.state(handle).unwrap();
    assert!(matches!(
        state_machine,
        LoadStateView::Failed {
            generation: 2,
            has_last_good: true,
            ..
        }
    ));
    store.shutdown().unwrap();
    store.shutdown().unwrap();

    write_events(
        json!([
            { "name": "initial_load_commit", "value": { "kind": "Bool", "value": initial_load_committed } },
            { "name": "cancel_idempotent", "value": { "kind": "Bool", "value": cancel_idempotent } },
            { "name": "worker_gpu_commit", "value": { "kind": "Bool", "value": worker_gpu_commit } },
            { "name": "state_machine", "value": { "kind": "Text", "value": "valid" } }
        ]),
        &[
            "canonical_path_normalized",
            "same_path_distinct_kind",
            "in_flight_request_deduplicated",
            "owned_payload_downcast",
            "initial_load_gpu_commit_on_caller",
            "failed_reload_kept_last_known_good",
            "cancel_repeated_and_late_result_rejected",
            "shutdown_idempotent",
        ],
    );
}
