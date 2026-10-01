use aether_engine::asset::material_asset::{resolve_material_result, MaterialAsset};
use aether_engine::asset::{
    AssetError, AssetId, AssetKind, AssetPayload, AssetStore, AssetStoreConfig, FrameBoundary,
    GpuAssetKey, GpuCommitCache, GpuCommitContext, LoadStateView,
};
use aether_engine::scene::config::MaterialConfig;
use aether_engine::visual_case::{ExpectedResult, ProbeValue, VisualManifest};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

static NEXT_ROOT: AtomicUsize = AtomicUsize::new(1);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "aether_t8_material_reload_{}_{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, path: &str, contents: impl AsRef<[u8]>) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn write_material(&self, roughness: f32) {
        let config = MaterialConfig {
            albedo: [0.3, 0.6, 0.9, 1.0],
            roughness,
            metallic: 0.25,
            albedo_texture: Some("textures/albedo.png".into()),
            ..MaterialConfig::default()
        };
        self.write("materials/paint.ron", ron::to_string(&config).unwrap());
    }

    fn write_texture(&self, valid: bool) {
        let path = self.0.join("materials/textures/albedo.png");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        if valid {
            image::RgbaImage::from_pixel(2, 2, image::Rgba([80, 120, 200, 255]))
                .save(path)
                .unwrap();
        } else {
            fs::write(path, b"corrupt png payload").unwrap();
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct CommitRecorder {
    keys: Vec<GpuAssetKey>,
    frames: Vec<u64>,
}

impl GpuCommitCache for CommitRecorder {
    fn commit_asset(
        &mut self,
        key: GpuAssetKey,
        _payload: Arc<dyn AssetPayload>,
        boundary: FrameBoundary,
    ) -> Result<(), AssetError> {
        self.keys.push(key);
        self.frames.push(boundary.frame_id);
        Ok(())
    }

    fn collect_retired(&mut self) {}
}

fn validate_case(workspace: &Path) {
    let source =
        fs::read_to_string(workspace.join("tests/cases/t8_material_reload_lkg.json")).unwrap();
    let manifest = VisualManifest::from_json_in(&source, workspace).unwrap();
    let materialized = manifest.materialize().unwrap();
    assert_eq!(materialized.len(), 1);
    assert_eq!(materialized[0].id, "t8_material_reload_lkg__default");
    match materialized[0].expected_result.as_ref().unwrap() {
        ExpectedResult::Render { probes, .. } => {
            let names: Vec<_> = probes.iter().map(|probe| probe.name.as_str()).collect();
            assert_eq!(
                names,
                [
                    "lkg_on_reload_error",
                    "generation_commit_boundary",
                    "stale_ticket_rejected"
                ]
            );
            assert!(probes
                .iter()
                .all(|probe| probe.value == ProbeValue::Bool(true)));
        }
        ExpectedResult::ExpectedError { .. } => panic!("T8.2 primary case must render"),
    }
}

fn apply_next(
    store: &mut AssetStore,
    slot: u32,
    root: &Path,
    first_frame: u64,
) -> (Result<aether_engine::asset::ApplyOutcome, AssetError>, u64) {
    let mut textures = aether_engine::asset::AssetManager::with_project_root(root);
    for offset in 0..500 {
        if let Some(result) = store.poll_results(8).unwrap().into_iter().next() {
            assert_eq!(result.request().slot, slot);
            let result = resolve_material_result(result, root, &mut textures);
            let frame = first_frame + offset;
            let applied = store.apply_result(result, FrameBoundary::new(frame));
            return (applied, frame);
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("MaterialAsset worker did not finish");
}

fn write_events(probes: Value, events: &[&str]) {
    let Some(path) = std::env::var_os("AETHER_T8_MATERIAL_EVENTS") else {
        return;
    };
    fs::write(
        PathBuf::from(path),
        serde_json::to_vec_pretty(&json!({
            "case_id": "t8_material_reload_lkg",
            "kind": "Fixture",
            "events": events,
            "probes": probes
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn t8_material_reload_lkg() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap();
    validate_case(workspace);

    let fixture = Fixture::new();
    fixture.write_material(0.42);
    fixture.write_texture(true);
    let mut store = AssetStore::new(AssetStoreConfig::new(&fixture.0));
    let (initial_handle, _) = store
        .request::<MaterialAsset>(Path::new("materials/paint.ron"))
        .unwrap();
    let (initial_result, initial_frame) =
        apply_next(&mut store, initial_handle.slot(), &fixture.0, 1);
    let initial = initial_result.unwrap();
    assert!(matches!(
        initial.state,
        LoadStateView::Ready { generation: 1 }
    ));
    let initial_material = store.get(initial_handle).unwrap();
    assert_eq!(initial_material.dependencies().len(), 1);
    assert_eq!(
        initial_material.dependencies()[0].asset.kind(),
        AssetKind::CpuTexture
    );

    // Resolve generation 2 but intentionally leave generation 1 uncommitted.
    fixture.write_material(0.73);
    let first_ticket = initial.ticket;
    let second_ticket = store.reload(initial_handle).unwrap();
    let (second_result, second_frame) = apply_next(
        &mut store,
        initial_handle.slot(),
        &fixture.0,
        initial_frame + 1,
    );
    let second = second_result.unwrap();
    assert_eq!(second.ticket, second_ticket);

    let mut recorder = CommitRecorder::default();
    let mut context = GpuCommitContext {
        cache: &mut recorder,
        boundary: FrameBoundary::new(second_frame + 2),
    };
    let stale_ticket_rejected = store
        .commit_gpu(first_ticket, &mut context)
        .is_err_and(|error| error == AssetError::StaleReloadTicket);
    store
        .commit_gpu(second.ticket.clone(), &mut context)
        .unwrap();
    let generation_commit_boundary = recorder.keys
        == [GpuAssetKey {
            asset: AssetId::from_path(
                AssetKind::Material,
                &fixture.0,
                Path::new("materials/paint.ron"),
            )
            .unwrap(),
            generation: 2,
        }]
        && recorder.frames == [second_frame + 2];
    let current_handle = store
        .current_handle::<MaterialAsset>(&second.ticket.asset)
        .unwrap();
    let last_good_roughness = store.get(current_handle).unwrap().config().roughness;

    // A corrupt dependency rejects generation 3 while generation 2 stays usable.
    fixture.write_texture(false);
    let failed_ticket = store.reload(current_handle).unwrap();
    let (failed_result, _) = apply_next(
        &mut store,
        current_handle.slot(),
        &fixture.0,
        second_frame + 3,
    );
    let failed = failed_result.unwrap();
    assert_eq!(failed.ticket, failed_ticket);
    assert!(matches!(
        failed.state,
        LoadStateView::Failed {
            generation: 3,
            has_last_good: true,
            ..
        }
    ));
    let lkg_on_reload_error =
        store.get(current_handle).unwrap().config().roughness == last_good_roughness;
    assert!(lkg_on_reload_error);
    assert!(generation_commit_boundary);
    assert!(stale_ticket_rejected);
    store.shutdown().unwrap();

    write_events(
        json!([
            { "name": "lkg_on_reload_error", "value": { "kind": "Bool", "value": lkg_on_reload_error } },
            { "name": "generation_commit_boundary", "value": { "kind": "Bool", "value": generation_commit_boundary } },
            { "name": "stale_ticket_rejected", "value": { "kind": "Bool", "value": stale_ticket_rejected } }
        ]),
        &[
            "material_ron_loaded_on_worker",
            "texture_dependency_resolved_by_t3",
            "reload_ticket_bound_to_material_generation",
            "gpu_commit_performed_at_frame_boundary",
            "stale_uncommitted_generation_rejected",
            "corrupt_texture_failure_preserved_last_known_good",
        ],
    );
}
