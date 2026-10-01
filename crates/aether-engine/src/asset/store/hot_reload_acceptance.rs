use super::*;
use crate::asset::{AssetError, AssetKind};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(1);
static SLOW_LOAD_STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Eq, PartialEq)]
struct TextAsset(String);

impl Asset for TextAsset {
    const KIND: AssetKind = AssetKind::GltfDocument;

    fn load(path: &Path) -> anyhow::Result<Self> {
        let value = fs::read_to_string(path)?;
        if value == "fail" {
            anyhow::bail!("fixture requested a decode failure");
        }
        if value.starts_with("slow:") {
            SLOW_LOAD_STARTED.store(true, Ordering::Release);
            thread::sleep(Duration::from_millis(250));
        }
        Ok(Self(value))
    }
}

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "aether_t8_hot_reload_{}_{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    fn write(&self, value: &str) {
        fs::write(self.0.join("reload.asset"), value).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn t8_hot_reload_shutdown_acceptance() {
    let fixture = Fixture::new();
    fixture.write("v1");
    let mut config = AssetStoreConfig::new(&fixture.0);
    config.join_timeout_ms = 0;
    let mut store = AssetStore::new(config);
    let (first, _) = store
        .request::<TextAsset>(Path::new("reload.asset"))
        .unwrap();
    let mut frame = 0;
    let first_outcome = finish_one(&mut store, first.slot(), &mut frame);
    assert!(matches!(first_outcome.state, LoadStateView::Ready { .. }));

    fixture.write("v2");
    store.reload(first).unwrap();
    let in_flight_retained = store.get(first).unwrap().0 == "v1";
    let second_outcome = finish_one(&mut store, first.slot(), &mut frame);
    assert!(matches!(second_outcome.state, LoadStateView::Ready { .. }));
    let current = store
        .current_handle::<TextAsset>(&second_outcome.asset)
        .unwrap();

    fixture.write("fail");
    store.reload(current).unwrap();
    let failure = finish_one(&mut store, current.slot(), &mut frame);
    let failed_reload_retained = matches!(
        failure.state,
        LoadStateView::Failed {
            has_last_good: true,
            ..
        }
    ) && store.get(current).is_ok_and(|asset| asset.0 == "v2");
    let old_generation_retained = in_flight_retained && failed_reload_retained;

    fixture.write("slow:worker");
    SLOW_LOAD_STARTED.store(false, Ordering::Release);
    store.reload(current).unwrap();
    let started_deadline = Instant::now() + Duration::from_secs(2);
    while !SLOW_LOAD_STARTED.load(Ordering::Acquire) && Instant::now() < started_deadline {
        thread::sleep(Duration::from_millis(1));
    }
    assert!(SLOW_LOAD_STARTED.load(Ordering::Acquire));
    let shutdown_started = Instant::now();
    let first_shutdown = store.shutdown();
    let shutdown_nonblocking = first_shutdown == Err(AssetError::ShutdownTimeout)
        && shutdown_started.elapsed() < Duration::from_millis(100);

    thread::sleep(Duration::from_millis(300));
    let worker_joined = store.shutdown().is_ok() && store.worker.is_none();

    write_events(shutdown_nonblocking, old_generation_retained, worker_joined);
    assert!(
        shutdown_nonblocking,
        "worker shutdown did not return within its configured bound"
    );
    assert!(
        old_generation_retained,
        "reload failure or in-flight reload hid the last good value"
    );
    assert!(
        worker_joined,
        "worker did not join after the running decode completed"
    );
}

fn finish_one(store: &mut AssetStore, slot: u32, frame: &mut u64) -> ApplyOutcome {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let results = store.poll_results(8).unwrap();
        for result in results {
            let target = result.request().slot == slot;
            *frame += 1;
            let outcome = store
                .apply_result(result, FrameBoundary::new(*frame))
                .unwrap();
            if target {
                return outcome;
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("timed out waiting for asset result");
}

fn write_events(shutdown_nonblocking: bool, old_generation_retained: bool, worker_joined: bool) {
    let Some(path) = std::env::var_os("AETHER_T8_HOT_RELOAD_EVENTS") else {
        return;
    };
    fs::write(
        Path::new(&path),
        serde_json::to_vec_pretty(&json!({
            "case_id": "t8_hot_reload_shutdown",
            "kind": "Fixture",
            "events": [
                "reload_in_flight_kept_current_generation_visible",
                "failed_generation_preserved_last_good_asset",
                "bounded_shutdown_timed_out_without_blocking",
                "worker_joined_on_idempotent_shutdown_retry"
            ],
            "probes": [
                { "name": "shutdown_nonblocking", "value": { "kind": "Bool", "value": shutdown_nonblocking } },
                { "name": "old_generation_retained", "value": { "kind": "Bool", "value": old_generation_retained } },
                { "name": "worker_joined", "value": { "kind": "Bool", "value": worker_joined } }
            ]
        }))
        .unwrap(),
    )
    .unwrap();
}
