use super::*;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "aether_project_index_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, relative: &str, contents: impl AsRef<[u8]>) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn scene_ron(name: &str) -> String {
    format!("(name: {name:?}, camera: (position: (0.0, 0.0, 0.0)))")
}

#[test]
fn scans_only_allowlisted_roots_with_stable_order_and_query_navigation() {
    let fixture = Fixture::new();
    fixture.write("scenes/zeta.ron", scene_ron("Zeta"));
    fixture.write("scenes/alpha.ron", scene_ron("Alpha"));
    fixture.write("scenes/ignored.txt", "not indexed");
    fixture.write("assets/models/ship.obj", "v 0 0 0\n");
    fixture.write("assets/models/ship.gltf", "{}");
    fixture.write("assets/textures/ship.png", b"png header");
    fixture.write("outside/secret.obj", "v 0 0 0\n");

    let snapshot = scan_project(&fixture.0).unwrap();
    let paths: Vec<_> = snapshot
        .entries
        .iter()
        .map(|item| item.path.as_str())
        .collect();
    assert_eq!(
        paths,
        [
            "assets/models/ship.gltf",
            "assets/models/ship.obj",
            "assets/textures/ship.png",
            "scenes/alpha.ron",
            "scenes/zeta.ron",
        ]
    );
    assert!(snapshot.missing_roots.is_empty());

    let query = ProjectQuery::default().search("ship");
    let found = query.apply(&snapshot);
    assert_eq!(found.len(), 3);
    assert!(query.kind(ResourceKind::Scene).apply(&snapshot).is_empty());

    let mut history = PathHistory::default();
    history.visit("assets");
    history.visit("assets/models");
    history.visit("scenes");
    assert_eq!(history.back().as_deref(), Some("assets/models"));
    assert_eq!(history.back().as_deref(), Some("assets"));
    assert_eq!(history.forward().as_deref(), Some("assets/models"));
    history.visit("assets/textures");
    assert!(history.forward().is_none());
}

#[test]
fn reports_missing_allowlisted_roots_without_scanning_project_fallbacks() {
    let fixture = Fixture::new();
    fixture.write("other/scene.ron", scene_ron("Out of scope"));
    let snapshot = scan_project(&fixture.0).unwrap();
    assert!(snapshot.entries.is_empty());
    assert_eq!(&*snapshot.missing_roots, &["assets", "scenes"]);
}

#[test]
fn classifies_typed_ron_and_rejects_defaultable_or_ambiguous_materials() {
    let fixture = Fixture::new();
    fixture.write("scenes/showcase.ron", scene_ron("Showcase"));
    fixture.write(
        "scenes/unknown-field.ron",
        "(name: \"Bad\", camera: (position: (0.0, 0.0, 0.0)), mystery: true)",
    );
    fixture.write("assets/prefabs/robot.ron", "(schema_version: 2, root: (instance_id: 1, name: \"Root\", components: [Transform(translation: (0.0, 0.0, 0.0), rotation_xyzw: (0.0, 0.0, 0.0, 1.0), scale: (1.0, 1.0, 1.0))], children: []))");
    fixture.write(
        "assets/materials/paint.ron",
        "(albedo_texture: Some(\"paint.png\"), roughness: 0.4)",
    );
    fixture.write("assets/materials/default.ron", "()");
    fixture.write("assets/misc/ambiguous.ron", "(roughness: 0.4)");
    fixture.write("assets/misc/broken.ron", "(");
    fixture.write("assets/misc/blank.ron", "   ");

    let snapshot = scan_project(&fixture.0).unwrap();
    let kinds: std::collections::BTreeMap<_, _> = snapshot
        .entries
        .iter()
        .map(|item| (item.path.as_str(), (item.kind, item.validity)))
        .collect();
    assert_eq!(
        kinds["scenes/showcase.ron"],
        (ResourceKind::Scene, ResourceValidity::Valid)
    );
    assert_eq!(
        kinds["scenes/unknown-field.ron"],
        (ResourceKind::Unknown, ResourceValidity::Unknown)
    );
    assert_eq!(
        kinds["assets/prefabs/robot.ron"],
        (ResourceKind::Prefab, ResourceValidity::Valid)
    );
    assert_eq!(
        kinds["assets/materials/paint.ron"],
        (ResourceKind::Material, ResourceValidity::Valid)
    );
    for path in ["assets/materials/default.ron", "assets/misc/ambiguous.ron"] {
        assert_eq!(kinds[path].0, ResourceKind::Unknown);
    }
    for path in ["assets/misc/broken.ron", "assets/misc/blank.ron"] {
        assert_eq!(kinds[path].1, ResourceValidity::Invalid);
    }
}

#[cfg(unix)]
#[test]
fn rejects_symlink_roots_and_skips_nested_symlinks_hidden_and_ignored_dirs() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    fixture.write("scenes/visible.ron", scene_ron("Visible"));
    fixture.write("assets/visible.obj", "v 0 0 0\n");
    fixture.write("assets/.hidden/secret.obj", "v 0 0 0\n");
    fixture.write("assets/target/secret.obj", "v 0 0 0\n");
    fixture.write("assets/tests/reports/secret.obj", "v 0 0 0\n");
    fixture.write("outside/escape.obj", "v 0 0 0\n");
    symlink(
        fixture.0.join("outside"),
        fixture.0.join("assets/link-outside"),
    )
    .unwrap();

    let snapshot = scan_project(&fixture.0).unwrap();
    let paths: Vec<_> = snapshot
        .entries
        .iter()
        .map(|item| item.path.as_str())
        .collect();
    assert_eq!(paths, ["assets/visible.obj", "scenes/visible.ron"]);

    let root_link = fixture.0.with_extension("symlink");
    symlink(&fixture.0, &root_link).unwrap();
    assert!(matches!(
        scan_project(&root_link),
        Err(IndexError::ProjectRootSymlink(_))
    ));
    let _ = fs::remove_file(root_link);

    let asset_root = fixture.0.join("assets");
    let moved = fixture.0.join("assets-real");
    fs::rename(&asset_root, &moved).unwrap();
    symlink(&moved, &asset_root).unwrap();
    assert!(matches!(
        scan_project(&fixture.0),
        Err(IndexError::ContentRootSymlink(_))
    ));
}

#[test]
fn refresh_handles_add_modify_delete_and_failed_scan_retains_snapshot() {
    let fixture = Fixture::new();
    fixture.write("scenes/one.ron", scene_ron("One"));
    let mut index = ProjectIndex::default();
    let first = index.begin_scan();
    let result = scan_project(&fixture.0);
    index.publish(first, result);
    assert_eq!(index.snapshot().entries.len(), 1);

    fixture.write("scenes/one.ron", "(");
    let second = index.begin_scan();
    index.publish(second, scan_project(&fixture.0));
    assert_eq!(index.snapshot().entries.len(), 1);
    assert_eq!(
        index.snapshot().entries[0].validity,
        ResourceValidity::Invalid
    );

    fixture.write("assets/new.obj", "v 0 0 0\n");
    fs::remove_file(fixture.0.join("scenes/one.ron")).unwrap();
    let third = index.begin_scan();
    index.publish(third, scan_project(&fixture.0));
    assert_eq!(index.snapshot().entries.len(), 1);
    assert_eq!(index.snapshot().entries[0].path, "assets/new.obj");

    let retained = index.snapshot().clone();
    let fourth = index.begin_scan();
    index.publish(
        fourth,
        Err(IndexError::ProjectRootSymlink(fixture.0.clone())),
    );
    assert_eq!(index.snapshot(), &retained);
    assert!(matches!(index.status(), IndexStatus::Error(_)));
}

#[test]
fn late_older_generation_cannot_replace_newer_snapshot() {
    let fixture = Fixture::new();
    fixture.write("scenes/new.ron", scene_ron("New"));
    let newer = scan_project(&fixture.0).unwrap();
    let older = IndexSnapshot {
        generation: 1,
        ..newer.clone()
    };
    let mut index = ProjectIndex::default();
    let old_request = index.begin_scan();
    let new_request = index.begin_scan();
    index.publish(new_request, Ok(newer));
    index.publish(old_request, Ok(older));
    assert_eq!(index.snapshot().generation, new_request);
    index.publish(old_request, Err(IndexError::Scan("late failure".into())));
    assert_eq!(index.snapshot().generation, new_request);
    assert_eq!(index.status(), &IndexStatus::Ready);
}
