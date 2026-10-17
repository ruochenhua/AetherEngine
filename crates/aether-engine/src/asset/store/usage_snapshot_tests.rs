use super::store_tests::{FixtureDir, TextAsset};
use super::*;
use crate::asset::{AssetKind, AssetManager};
use std::thread;
use std::time::{Duration, Instant};

struct TextureText;

impl Asset for TextureText {
    const KIND: AssetKind = AssetKind::CpuTexture;

    fn load(path: &Path) -> anyhow::Result<Self> {
        std::fs::read_to_string(path)?;
        Ok(Self)
    }
}

struct MaterialText;

impl Asset for MaterialText {
    const KIND: AssetKind = AssetKind::Material;

    fn load(path: &Path) -> anyhow::Result<Self> {
        std::fs::read_to_string(path)?;
        Ok(Self)
    }
}

fn result(store: &mut AssetStore) -> AssetResult {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(result) = store.poll_results(1).unwrap().pop() {
            return result;
        }
        assert!(Instant::now() < deadline, "worker result timed out");
        thread::sleep(Duration::from_millis(1));
    }
}

fn apply_next(store: &mut AssetStore, frame: u64) -> ApplyOutcome {
    let result = result(store);
    store
        .apply_result(result, FrameBoundary::new(frame))
        .unwrap()
}

fn request_usage(request: &LoadRequest) -> AssetRequestUsage {
    AssetRequestUsage {
        generation: request.generation,
        from_generation: request.from_generation,
        sequence: request.sequence,
        cancelled: request.cancel.is_cancelled(),
    }
}

#[test]
fn usage_snapshot_manager_retains_sync_ownership_after_handle_leaves_scope() {
    let fixture = FixtureDir::new();
    let path = fixture.write("sync.asset", "value");
    let mut manager = AssetManager::with_project_root(path.parent().unwrap());
    let id = {
        let handle = manager.load::<TextAsset>("./sync.asset").unwrap();
        manager.asset_id(handle).unwrap()
    };
    assert_eq!(
        manager.resource_usage_snapshot(),
        vec![AssetUsageSnapshot {
            asset: id,
            retained: Some(RetainedAssetUsage {
                current_generation: Some(1),
                requested_generation: 1,
                state: LoadStateView::Ready { generation: 1 },
                retired_generations: vec![],
                pending: None,
            }),
            deferred_results: vec![],
        }]
    );
}

#[test]
fn usage_snapshot_reports_pending_request_without_loaded_payload() {
    let fixture = FixtureDir::new();
    fixture.write("pending.asset", "value");
    let mut store = fixture.store();
    let (handle, _) = store
        .request::<TextAsset>(Path::new("pending.asset"))
        .unwrap();
    let snapshot = store.resource_usage_snapshot();
    assert_eq!(snapshot.len(), 1);
    let retained = snapshot[0].retained.as_ref().unwrap();
    assert_eq!(retained.current_generation, None);
    assert_eq!(retained.requested_generation, handle.generation());
    assert_eq!(retained.state, store.state(handle).unwrap());
    assert_eq!(
        retained.pending,
        Some(AssetRequestUsage {
            generation: 1,
            from_generation: 0,
            sequence: 1,
            cancelled: false,
        })
    );
    assert!(retained.retired_generations.is_empty());
    store.shutdown().unwrap();
}

#[test]
fn usage_snapshot_distinguishes_failed_initial_load_from_last_good_reload() {
    let fixture = FixtureDir::new();
    fixture.write("good.asset", "good");
    fixture.write("bad.asset", "fail");
    let mut store = fixture.store();
    let good = store
        .load_sync::<TextAsset>(Path::new("good.asset"))
        .unwrap();
    let (bad, _) = store.request::<TextAsset>(Path::new("bad.asset")).unwrap();
    apply_next(&mut store, 1);
    fixture.write("good.asset", "fail");
    store.reload(good).unwrap();
    apply_next(&mut store, 2);
    let snapshot = store.resource_usage_snapshot();
    assert_eq!(snapshot.len(), 2);
    for (usage, handle, current, has_last_good) in [
        (&snapshot[0], bad, None, false),
        (&snapshot[1], good, Some(1), true),
    ] {
        let retained = usage.retained.as_ref().unwrap();
        assert_eq!(retained.current_generation, current);
        assert_eq!(retained.state, store.state(handle).unwrap());
        assert!(matches!(retained.state, LoadStateView::Failed {
            has_last_good: actual, ..
        } if actual == has_last_good));
        assert!(retained.pending.is_none());
        assert!(retained.retired_generations.is_empty());
    }
    assert_eq!(
        snapshot[0].retained.as_ref().unwrap().requested_generation,
        1
    );
    assert_eq!(
        snapshot[1].retained.as_ref().unwrap().requested_generation,
        2
    );
    assert_eq!(store.get(good).unwrap().0, "good");
    store.shutdown().unwrap();
}

#[test]
fn usage_snapshot_reports_current_retired_and_pending_generations_separately() {
    let fixture = FixtureDir::new();
    fixture.write("reload.asset", "first");
    let mut store = fixture.store();
    let old = store
        .load_sync::<TextAsset>(Path::new("reload.asset"))
        .unwrap();
    let id = store.asset_id_for_handle(old).unwrap();
    for frame in 1..=2 {
        let current = store.current_handle::<TextAsset>(&id).unwrap();
        fixture.write("reload.asset", &format!("value {frame}"));
        store.reload(current).unwrap();
        apply_next(&mut store, frame);
    }
    let current = store.current_handle::<TextAsset>(&id).unwrap();
    let reload = store.reload(current).unwrap();
    let snapshot = store.resource_usage_snapshot();
    assert_eq!(snapshot.len(), 1);
    let retained = snapshot[0].retained.as_ref().unwrap();
    assert_eq!(retained.current_generation, Some(3));
    assert_eq!(retained.retired_generations, vec![1, 2]);
    assert_eq!(retained.requested_generation, 4);
    assert_eq!(retained.state, LoadStateView::Loading { generation: 4 });
    assert_eq!(retained.pending.as_ref().unwrap().sequence, reload.sequence);
    assert_eq!(retained.pending.as_ref().unwrap().from_generation, 3);
    assert_eq!(retained.pending.as_ref().unwrap().generation, 4);
    store.shutdown().unwrap();
}

#[test]
fn usage_snapshot_includes_orphan_deferred_identity_and_failed_result() {
    let fixture = FixtureDir::new();
    fixture.write("foreign.asset", "value");
    fixture.write("failed.asset", "fail");
    let mut source = fixture.store();
    source
        .request::<TextAsset>(Path::new("foreign.asset"))
        .unwrap();
    source
        .request::<TextAsset>(Path::new("failed.asset"))
        .unwrap();
    let ready = result(&mut source);
    let failed = result(&mut source);
    let ready_id = ready.request().asset.clone();
    let failed_id = failed.request().asset.clone();
    let ready_usage = request_usage(ready.request());
    let failed_usage = request_usage(failed.request());
    let error = match &failed {
        AssetResult::Failed { error, .. } => error.clone(),
        _ => panic!("expected decode failure"),
    };
    let mut destination = fixture.store();
    destination.defer_result(ready);
    destination.defer_result(failed);
    assert_eq!(
        destination.resource_usage_snapshot(),
        vec![
            AssetUsageSnapshot {
                asset: failed_id,
                retained: None,
                deferred_results: vec![DeferredAssetUsage {
                    request: failed_usage,
                    state: DeferredAssetState::Failed { error },
                }],
            },
            AssetUsageSnapshot {
                asset: ready_id,
                retained: None,
                deferred_results: vec![DeferredAssetUsage {
                    request: ready_usage,
                    state: DeferredAssetState::Ready,
                }],
            },
        ]
    );
    assert_eq!(destination.poll_results(10).unwrap().len(), 2);
    source.shutdown().unwrap();
}

#[test]
fn usage_snapshot_orders_typed_normalized_ids_independently_of_insertion() {
    let fixture = FixtureDir::new();
    fixture.write("a.asset", "a");
    fixture.write("z.asset", "z");
    let ids = [
        "material:z.asset",
        "gltf_document:z.asset",
        "material:a.asset",
        "cpu_texture:a.asset",
        "gltf_document:a.asset",
    ];
    let build = |reverse: bool| {
        let mut store = fixture.store();
        let mut input = ids.to_vec();
        if reverse {
            input.reverse();
        }
        for id in input {
            let id: AssetId = id.parse().unwrap();
            let normalized_input = format!("./nested/../{}", id.path());
            let path = Path::new(&normalized_input);
            match id.kind() {
                AssetKind::CpuTexture => {
                    store.load_sync::<TextureText>(path).unwrap();
                }
                AssetKind::GltfDocument => {
                    store.load_sync::<TextAsset>(path).unwrap();
                }
                AssetKind::Material => {
                    store.load_sync::<MaterialText>(path).unwrap();
                }
                _ => unreachable!(),
            };
        }
        store
    };
    let first = build(false).resource_usage_snapshot();
    let second = build(true).resource_usage_snapshot();
    assert_eq!(first.len(), 5);
    assert_eq!(first, second);
    assert_eq!(
        first
            .iter()
            .map(|usage| usage.asset.to_string())
            .collect::<Vec<_>>(),
        vec![
            "cpu_texture:a.asset",
            "gltf_document:a.asset",
            "gltf_document:z.asset",
            "material:a.asset",
            "material:z.asset",
        ]
    );
    assert_eq!(first[0].asset.kind(), AssetKind::CpuTexture);
}

#[test]
fn usage_snapshot_reports_cancelled_deferred_generation_after_sync_replacement() {
    let fixture = FixtureDir::new();
    fixture.write("replaced.asset", "value");
    let mut store = fixture.store();
    let (_, ticket) = store
        .request::<TextAsset>(Path::new("replaced.asset"))
        .unwrap();
    let deferred = result(&mut store);
    let ready = store
        .load_sync::<TextAsset>(Path::new("replaced.asset"))
        .unwrap();
    store.defer_result(deferred);
    let snapshot = store.resource_usage_snapshot();
    assert_eq!(snapshot.len(), 1);
    let retained = snapshot[0].retained.as_ref().unwrap();
    assert_eq!(retained.current_generation, Some(2));
    assert_eq!(retained.requested_generation, 2);
    assert!(retained.pending.is_none());
    assert_eq!(snapshot[0].deferred_results.len(), 1);
    assert_eq!(snapshot[0].deferred_results[0].request.generation, 1);
    assert!(snapshot[0].deferred_results[0].request.cancelled);
    assert_eq!(
        snapshot[0].deferred_results[0].state,
        DeferredAssetState::Ready
    );
    assert_eq!(store.get(ready).unwrap().0, "value");
    store.cancel(&ticket).unwrap();
    store.shutdown().unwrap();
}

#[test]
fn usage_snapshot_repeated_reads_preserve_queues_payloads_states_and_handles() {
    let fixture = FixtureDir::new();
    fixture.write("deferred.asset", "deferred");
    fixture.write("queued.asset", "queued");
    let mut store = fixture.store();
    let loaded = store
        .load_sync::<TextAsset>(Path::new("deferred.asset"))
        .unwrap();
    let payload = store.get(loaded).unwrap();
    let reload = store.reload(loaded).unwrap();
    let deferred = result(&mut store);
    let deferred_sequence = deferred.request().sequence;
    store.defer_result(deferred);
    let (queued, _) = store
        .request::<TextAsset>(Path::new("queued.asset"))
        .unwrap();
    // Wait for a real result to enter the worker channel without draining it.
    store.job_tx.take();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !store.worker.as_ref().unwrap().is_finished() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    let states = (store.state(loaded).unwrap(), store.state(queued).unwrap());
    let counts = (
        store.entries.len(),
        store.tickets.len(),
        store.deferred_results.len(),
    );
    let counters = (store.next_slot, store.next_ticket, store.next_sequence);
    let boundaries = (store.last_applied_boundary, store.last_gpu_boundary);
    let strong = Arc::strong_count(&payload);
    let snapshot = store.resource_usage_snapshot();
    assert_eq!(snapshot.len(), 2);
    assert_eq!(snapshot[0].deferred_results.len(), 1);
    assert_eq!(
        snapshot[0].retained.as_ref().unwrap().current_generation,
        Some(1)
    );
    assert_eq!(
        snapshot[0]
            .retained
            .as_ref()
            .unwrap()
            .pending
            .as_ref()
            .unwrap()
            .generation,
        2
    );
    for _ in 0..5 {
        assert_eq!(snapshot, store.resource_usage_snapshot());
    }
    assert_eq!(
        states,
        (store.state(loaded).unwrap(), store.state(queued).unwrap())
    );
    assert_eq!(
        counts,
        (
            store.entries.len(),
            store.tickets.len(),
            store.deferred_results.len()
        )
    );
    assert_eq!(
        counters,
        (store.next_slot, store.next_ticket, store.next_sequence)
    );
    assert_eq!(
        boundaries,
        (store.last_applied_boundary, store.last_gpu_boundary)
    );
    assert_eq!(Arc::strong_count(&payload), strong);
    assert!(Arc::ptr_eq(&payload, &store.get(loaded).unwrap()));
    assert_eq!(store.load_ticket(&reload).unwrap(), reload.load_ticket);
    assert!(store.worker.is_some());
    let results = store.poll_results(10).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].request().sequence, deferred_sequence);
    assert_eq!(results[1].request().slot, queued.slot());
    for result in results {
        store.apply_result(result, FrameBoundary::new(1)).unwrap();
    }
    let current = store
        .current_handle::<TextAsset>(&snapshot[0].asset)
        .unwrap();
    assert_eq!(current.generation(), 2);
    assert_eq!(
        store.state(current),
        Ok(LoadStateView::Ready { generation: 2 })
    );
    assert_eq!(store.get(loaded), Err(AssetError::StaleHandle));
    assert_eq!(store.get(queued).unwrap().0, "queued");
    store.shutdown().unwrap();
}
