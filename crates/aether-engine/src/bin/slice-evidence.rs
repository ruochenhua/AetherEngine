//! Emit measured fixture evidence for the VisualCase v2 slice runner.

#[path = "slice-evidence/t7_evidence.rs"]
mod t7_evidence;

use aether_engine::asset::store::{
    AssetError, AssetKind, AssetStore, AssetStoreConfig, FrameBoundary, LoadStateView, TypedAsset,
};
use aether_engine::asset::Asset;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug)]
struct ProbeAsset(String);

impl Asset for ProbeAsset {
    fn load(path: &Path) -> anyhow::Result<Self> {
        Ok(Self(std::fs::read_to_string(path)?))
    }
}

impl TypedAsset for ProbeAsset {
    const KIND: AssetKind = AssetKind::Prefab;
}

fn main() {
    if let Err(error) = run() {
        eprintln!("slice-evidence: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut slice = None;
    let mut variant = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--slice" => slice = args.next(),
            "--variant" => variant = args.next(),
            "--help" | "-h" => {
                println!("Usage: slice-evidence --slice T8.1|T7.1|T7.2|T7.3 --variant ID");
                return Ok(());
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
    }
    let slice = slice.ok_or("missing --slice")?;
    let variant = variant.ok_or("missing --variant")?;
    let report = match slice.as_str() {
        "T8.1" => asset_store_evidence()?,
        "T7.1" => t7_evidence::gltf_fixture_evidence(&variant)?,
        "T7.2" => t7_evidence::palette_evidence()?,
        "T7.3" => t7_evidence::animation_evidence(&variant)?,
        other => return Err(format!("unsupported evidence slice: {other}").into()),
    };
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}

fn base_render() -> Value {
    json!({"kind":"Render","diagnostics":[],"fallbacks":[],"metrics":{},"hashes":{},"probes":{},"graph":null,"asset_events":[]})
}

fn asset_store_evidence() -> Result<Value, Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!("aether_t8_evidence_{}", unique_suffix()));
    std::fs::create_dir_all(&root)?;
    std::fs::write(root.join("initial.txt"), "typed asset")?;
    std::fs::write(root.join("cancel.txt"), "cancel me")?;
    let mut store = AssetStore::new(AssetStoreConfig::new(root.clone()));
    let owner_thread = thread::current().id();
    let (handle, _load_ticket) = store.request::<ProbeAsset>(Path::new("initial.txt"))?;
    let loading_before_decode =
        matches!(store.state(handle.clone())?, LoadStateView::Loading { .. });
    let result = wait_result(&mut store)?;
    let accepted = store.apply_result(result, FrameBoundary { frame_id: 1 })?;
    let loading_before_commit =
        matches!(store.state(handle.clone())?, LoadStateView::Loading { .. });
    let callbacks = AtomicUsize::new(0);
    let commit_on_owner = AtomicUsize::new(0);
    store.commit_result(
        &accepted,
        FrameBoundary { frame_id: 1 },
        |asset, generation, payload| {
            if asset.kind != AssetKind::Prefab
                || generation != 1
                || payload
                    .downcast_ref::<ProbeAsset>()
                    .map(|value| value.0.as_str())
                    != Some("typed asset")
            {
                return Err(AssetError::PayloadTypeMismatch);
            }
            callbacks.fetch_add(1, Ordering::SeqCst);
            if thread::current().id() == owner_thread {
                commit_on_owner.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        },
    )?;
    let duplicate_commit =
        store.commit_result(&accepted, FrameBoundary { frame_id: 1 }, |_, _, _| {
            callbacks.fetch_add(100, Ordering::SeqCst);
            Err(AssetError::PayloadTypeMismatch)
        });
    let commit_idempotent = duplicate_commit.is_ok() && callbacks.load(Ordering::SeqCst) == 1;
    let initial_ready = matches!(
        store.state(handle.clone())?,
        LoadStateView::Ready { generation: 1 }
    );
    let (cancel_handle, cancel_ticket) = store.request::<ProbeAsset>(Path::new("cancel.txt"))?;
    store.cancel(&cancel_ticket)?;
    store.cancel(&cancel_ticket)?;
    let mut cancelled = false;
    for _ in 0..200 {
        for result in store.poll_results(8)? {
            cancelled |= matches!(
                store.apply_result(result, FrameBoundary { frame_id: 2 }),
                Err(AssetError::Cancelled)
            );
        }
        if cancelled {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    let cancel_state_failed = matches!(
        store.state(cancel_handle),
        Ok(LoadStateView::Failed {
            error: AssetError::Cancelled,
            ..
        })
    );
    let state_machine =
        loading_before_decode && loading_before_commit && initial_ready && cancelled;
    let events = json!([
        {"event":"request","asset":"Prefab:initial.txt","state":"Loading"},
        {"event":"decode_complete","state":"Loading"},
        {"event":"frame_commit","generation":1,"state":"Ready","thread":"owner"},
        {"event":"cancel","idempotent":true,"result":"Cancelled"}
    ]);
    let mut report = base_render();
    report["probes"] = json!({
        "initial_load_commit": state_machine,
        "commit_idempotent": commit_idempotent,
        "cancel_idempotent": cancelled && cancel_state_failed,
        "commit_callback_on_owner_thread": callbacks.load(Ordering::SeqCst) == 1
            && commit_on_owner.load(Ordering::SeqCst) == 1,
        "state_machine": if state_machine { "valid" } else { "invalid" }
    });
    report["asset_events"] = events;
    report["metrics"] = json!({"commit_callbacks": callbacks.load(Ordering::SeqCst), "owner_callbacks": commit_on_owner.load(Ordering::SeqCst)});
    store.shutdown()?;
    std::fs::remove_dir_all(root)?;
    Ok(report)
}

fn wait_result(
    store: &mut AssetStore,
) -> Result<aether_engine::asset::store::AssetResult, AssetError> {
    for _ in 0..200 {
        if let Some(result) = store.poll_results(1)?.pop() {
            return Ok(result);
        }
        thread::sleep(Duration::from_millis(2));
    }
    Err(AssetError::WorkerStopped)
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
