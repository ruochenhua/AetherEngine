use super::store_tests::{FixtureDir, TextAsset};
use super::*;
use std::path::Path;
use std::thread;
use std::time::Duration;

#[test]
fn earlier_frame_boundary_defers_result_for_a_later_retry() {
    let fixture = FixtureDir::new();
    fixture.write("model.gltf", "version one");
    let mut store = fixture.store();
    let (handle, _) = store.request::<TextAsset>(Path::new("model.gltf")).unwrap();
    let first_result = wait_result(&mut store);
    store
        .apply_result(first_result, FrameBoundary::new(5))
        .unwrap();

    fixture.write("model.gltf", "version two");
    store.reload(handle).unwrap();
    let second_result = wait_result(&mut store);
    assert_eq!(
        store.apply_result(second_result, FrameBoundary::new(4)),
        Err(AssetError::InvalidFrameBoundary)
    );
    let deferred = store.poll_results(1).unwrap().pop().unwrap();
    store.apply_result(deferred, FrameBoundary::new(5)).unwrap();
    store.shutdown().unwrap();
}

fn wait_result(store: &mut AssetStore) -> AssetResult {
    for _ in 0..500 {
        if let Some(result) = store.poll_results(1).unwrap().into_iter().next() {
            return result;
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("asset worker did not produce a result")
}
