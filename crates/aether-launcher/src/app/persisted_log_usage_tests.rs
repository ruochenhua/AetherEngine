use super::*;
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(1);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("aether_ew5_b6_{}_{}", std::process::id(), id));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("active.ron"), "scene").unwrap();
        Self(root)
    }

    fn scene(&self) -> PathBuf {
        self.0.join("active.ron")
    }

    fn log(&self) -> PathBuf {
        super::super::editor::operation_log_path(&self.scene())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn capture(scene: Option<&Path>, root: &Path) -> PersistedLogResourceSnapshot {
    capture_persisted_log_resource_usage(scene, root)
}

fn operation_line(sequence: u64) -> String {
    json!({
        "schema_version": 2,
        "sequence": sequence,
        "operation": {
            "op": "place_object",
            "object": {
                "name": "Ship",
                "mesh": {"File": "assets/ship.obj"},
                "material": {"albedo_texture": "assets/shared.png"},
                "material_asset": "assets/shared.png",
                "visible": true
            }
        }
    })
    .to_string()
}

#[test]
fn no_active_scene_is_not_applicable() {
    let f = Fixture::new();
    let snapshot = capture(None, &f.0);
    assert_eq!(snapshot.state, PersistedLogState::NotApplicable);
    assert_eq!(snapshot.source_path, None);
    assert!(snapshot.resources.is_empty());
}

#[test]
fn missing_log_is_known_empty_and_keeps_derived_source_path() {
    let f = Fixture::new();
    let snapshot = capture(Some(&f.scene()), &f.0);
    assert_eq!(snapshot.state, PersistedLogState::Missing);
    assert_eq!(snapshot.source_path.as_deref(), Some(f.log().as_path()));
    assert!(snapshot.resources.is_empty());
}

#[test]
fn empty_valid_log_is_complete_with_zero_resources() {
    let f = Fixture::new();
    fs::write(f.log(), "\n  \n").unwrap();
    let snapshot = capture(Some(&f.scene()), &f.0);
    assert_eq!(snapshot.state, PersistedLogState::Complete);
    assert!(snapshot.resources.is_empty());
}

#[test]
fn complete_log_keeps_typed_duplicate_references_and_is_deterministic() {
    let f = Fixture::new();
    let content = format!("{}\n{}\n", operation_line(0), operation_line(1));
    fs::write(f.log(), &content).unwrap();
    let first = capture(Some(&f.scene()), &f.0);
    assert_eq!(first.state, PersistedLogState::Complete);
    assert_eq!(first.source_path.as_deref(), Some(f.log().as_path()));
    assert_eq!(first.resources.len(), 6);
    assert_eq!(
        first
            .resources
            .iter()
            .filter(|usage| usage.authored_path == "assets/shared.png")
            .count(),
        4,
        "duplicate path spellings in distinct operation fields must be retained"
    );
    assert!(first
        .resources
        .iter()
        .any(|usage| usage.source.sequence == Some(1)));
    assert_eq!(first, capture(Some(&f.scene()), &f.0));
}

#[test]
fn malformed_jsonl_and_unknown_schema_are_unavailable() {
    let f = Fixture::new();
    fs::write(f.log(), "{not-json}\n").unwrap();
    let malformed = capture(Some(&f.scene()), &f.0);
    assert!(matches!(malformed.state, PersistedLogState::Unavailable(_)));
    assert!(malformed.resources.is_empty());
    fs::write(f.log(), "{\"schema_version\":99}\n").unwrap();
    let schema = capture(Some(&f.scene()), &f.0);
    assert!(matches!(schema.state, PersistedLogState::Unavailable(_)));
}

#[test]
fn unreadable_log_target_is_unavailable() {
    let f = Fixture::new();
    fs::create_dir(f.log()).unwrap();
    let snapshot = capture(Some(&f.scene()), &f.0);
    assert!(matches!(snapshot.state, PersistedLogState::Unavailable(_)));
}

#[cfg(unix)]
#[test]
fn fifo_log_target_is_unavailable_without_blocking_on_open() {
    use std::{ffi::CString, io::Write, os::unix::fs::OpenOptionsExt, thread, time::Duration};
    let f = Fixture::new();
    let path = CString::new(f.log().as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);

    let writer_path = f.log();
    let writer = thread::spawn(move || {
        for _ in 0..50 {
            match fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(writer_path.as_path())
            {
                Ok(mut fifo) => {
                    fifo.write_all(operation_line(0).as_bytes()).unwrap();
                    return;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotConnected => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) if error.raw_os_error() == Some(libc::ENXIO) => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("could not open FIFO writer: {error}"),
            }
        }
    });
    let snapshot = capture(Some(&f.scene()), &f.0);
    writer.join().unwrap();
    assert!(matches!(snapshot.state, PersistedLogState::Unavailable(_)));
}

#[test]
fn opened_file_identity_mismatch_is_rejected_deterministically() {
    let f = Fixture::new();
    let first = f.0.join("first");
    let second = f.0.join("second");
    fs::write(&first, "first").unwrap();
    fs::write(&second, "second").unwrap();
    let expected = fs::File::open(first).unwrap();
    let opened = fs::File::open(second).unwrap();
    assert!(validate_opened_file_identity(&expected, &opened).is_err());
}

#[test]
fn scene_outside_project_root_is_unavailable_without_reading_adjacent_log() {
    let f = Fixture::new();
    let outside = f.0.with_extension("outside");
    fs::create_dir_all(&outside).unwrap();
    let scene = outside.join("active.ron");
    fs::write(&scene, "scene").unwrap();
    fs::write(
        super::super::editor::operation_log_path(&scene),
        operation_line(0),
    )
    .unwrap();
    let snapshot = capture(Some(&scene), &f.0);
    assert!(matches!(snapshot.state, PersistedLogState::Unavailable(_)));
    assert!(snapshot.resources.is_empty());
    fs::remove_dir_all(outside).unwrap();
}

#[cfg(unix)]
#[test]
fn symlink_log_escaping_project_root_is_unavailable() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let outside = f.0.with_extension("outside");
    fs::create_dir_all(&outside).unwrap();
    let outside_log = outside.join("outside.editor.jsonl");
    fs::write(&outside_log, operation_line(0)).unwrap();
    symlink(&outside_log, f.log()).unwrap();
    let snapshot = capture(Some(&f.scene()), &f.0);
    assert!(matches!(snapshot.state, PersistedLogState::Unavailable(_)));
    assert!(snapshot.resources.is_empty());
    fs::remove_dir_all(outside).unwrap();
}

#[cfg(unix)]
#[test]
fn dangling_log_symlink_is_unavailable_not_missing() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    symlink(f.0.join("not-created.jsonl"), f.log()).unwrap();
    let snapshot = capture(Some(&f.scene()), &f.0);
    assert!(matches!(snapshot.state, PersistedLogState::Unavailable(_)));
}

#[test]
fn complete_log_snapshot_does_not_change_file_bytes_or_mtime() {
    let f = Fixture::new();
    let content = format!("{}\n", operation_line(0));
    fs::write(f.log(), &content).unwrap();
    let before = fs::metadata(f.log()).unwrap().modified().unwrap();
    let snapshot = capture(Some(&f.scene()), &f.0);
    let after = fs::metadata(f.log()).unwrap().modified().unwrap();
    assert_eq!(snapshot.state, PersistedLogState::Complete);
    assert_eq!(fs::read_to_string(f.log()).unwrap(), content);
    assert_eq!(before, after);
}
