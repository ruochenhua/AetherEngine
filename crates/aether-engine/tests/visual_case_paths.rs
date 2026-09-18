#[path = "support/visual_case.rs"]
mod fixtures;

use aether_engine::visual_case::VisualManifest;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct ProjectTree(PathBuf);

impl ProjectTree {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "aether-t01-paths-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::create_dir_all(path.join("scenes")).unwrap();
        std::fs::create_dir_all(path.join("tests/reference")).unwrap();
        std::fs::write(path.join("scenes/01_deferred.ron"), "fixture").unwrap();
        Self(path)
    }

    fn parse(
        &self,
        value: Value,
    ) -> Result<VisualManifest, aether_engine::visual_case::VisualCaseError> {
        VisualManifest::from_json_in(&json!([value]).to_string(), &self.0)
    }
}

impl Drop for ProjectTree {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn root_aware_paths_require_resolvable_containment_and_allow_pending_leaf() {
    let tree = ProjectTree::new();
    tree.parse(fixtures::case("valid")).unwrap(); // Pending reference file need not exist yet.
    for (pointer, path) in [
        ("/scene", "scenes/missing.ron"),
        ("/scene", "scenes"),
        ("/scene", "scenes/../outside.ron"),
        ("/scene", "scenes\\outside.ron"),
        (
            "/reference/path",
            "tests/reference/missing-parent/image.png",
        ),
        ("/reference/path", "tests/reference"),
        ("/reference/path", "/tmp/image.png"),
    ] {
        let mut fixture = fixtures::case("invalid");
        *fixture.pointer_mut(pointer).unwrap() = json!(path);
        assert!(tree.parse(fixture).is_err(), "accepted {path}");
    }
    let input = json!([fixtures::case("valid")]).to_string();
    assert!(VisualManifest::from_json_in(&input, &tree.0.join("missing-root")).is_err());
    std::fs::remove_dir(tree.0.join("tests/reference")).unwrap();
    assert!(tree.parse(fixtures::case("missing-root")).is_err());
}

#[test]
fn complete_reference_provenance_requires_an_existing_file() {
    let tree = ProjectTree::new();
    let mut fixture = fixtures::case("release");
    fixture["reference"] = json!({"path":"tests/reference/release.png","sha256":"a".repeat(64),
        "source_commit":"b".repeat(40),"os":"macOS","driver":"Metal","adapter":"GPU",
        "runner_version":"2","width":1280,"height":720,"format":"png-rgba8"});
    assert!(tree.parse(fixture.clone()).is_err());
    std::fs::write(tree.0.join("tests/reference/release.png"), "fixture").unwrap();
    tree.parse(fixture).unwrap();
}

#[cfg(unix)]
#[test]
fn canonical_paths_reject_symlink_escapes_and_dangling_links() {
    use std::os::unix::fs::symlink;
    let tree = ProjectTree::new();
    let outside = ProjectTree::new();
    symlink(outside.0.join("scenes"), tree.0.join("scenes/link")).unwrap();
    symlink(
        outside.0.join("tests/reference"),
        tree.0.join("tests/reference/link"),
    )
    .unwrap();
    symlink(
        outside.0.join("missing"),
        tree.0.join("tests/reference/dangling.png"),
    )
    .unwrap();
    symlink("cycle.png", tree.0.join("tests/reference/cycle.png")).unwrap();
    symlink(
        outside.0.join("scenes/01_deferred.ron"),
        tree.0.join("tests/reference/external.png"),
    )
    .unwrap();
    for (pointer, path) in [
        ("/scene", "scenes/link/01_deferred.ron"),
        ("/reference/path", "tests/reference/link/future.png"),
        ("/reference/path", "tests/reference/dangling.png"),
        ("/reference/path", "tests/reference/cycle.png"),
        ("/reference/path", "tests/reference/external.png"),
    ] {
        let mut fixture = fixtures::case("escape");
        *fixture.pointer_mut(pointer).unwrap() = json!(path);
        assert!(tree.parse(fixture).is_err(), "accepted symlink {path}");
    }
    symlink("01_deferred.ron", tree.0.join("scenes/internal.ron")).unwrap();
    let mut internal = fixtures::case("internal");
    internal["scene"] = json!("scenes/internal.ron");
    tree.parse(internal).unwrap();
    std::fs::rename(tree.0.join("scenes"), tree.0.join("old-scenes")).unwrap();
    symlink(outside.0.join("scenes"), tree.0.join("scenes")).unwrap();
    assert!(tree.parse(fixtures::case("root-escape")).is_err());
}

#[cfg(unix)]
#[test]
fn materialization_revalidates_child_reference_and_current_filesystem() {
    use std::os::unix::fs::symlink;
    let tree = ProjectTree::new();
    let outside = ProjectTree::new();
    let mut fixture = fixtures::case("base");
    fixture["variants"] = json!([fixtures::variant("child")]);
    let manifest = tree.parse(fixture).unwrap();
    manifest.materialize().unwrap();
    symlink(
        outside.0.join("scenes/01_deferred.ron"),
        tree.0.join("tests/reference/base__child.png"),
    )
    .unwrap();
    assert!(manifest.materialize().is_err());
    let base = tree.parse(fixtures::case("no-variants")).unwrap();
    std::fs::remove_file(tree.0.join("scenes/01_deferred.ron")).unwrap();
    assert!(base.materialize().is_err());
}
