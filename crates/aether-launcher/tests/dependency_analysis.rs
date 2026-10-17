use aether_launcher::project_dependencies::{
    analyze, AnalysisBudget, ReferenceKind, Resolution, SourceLocator,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "aether-dependencies-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("assets")).unwrap();
        Self(root)
    }
    fn write(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fn report(&self, target: &str) -> aether_launcher::project_dependencies::DependencyReport {
        analyze(&self.0, target, AnalysisBudget::default()).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn scene(extra: &str) -> String {
    format!(r#"(name:"assets/decoy.png",camera:(position:(0.0,0.0,0.0)),{extra})"#)
}

#[test]
fn scene_objects_preserve_distinct_typed_edges_and_ignore_names() {
    let f = Fixture::new();
    let source = scene(
        r#"objects:[(name:"one",mesh:File("assets/a.bin"),material_asset:Some("assets/m.ron")),(name:"two",mesh:File("assets/a.bin"))]"#,
    );
    f.write("scenes/a.ron", &source);
    f.write("assets/a.bin", b"mesh");
    f.write("assets/m.ron", "(roughness:0.5)");
    let parsed = aether_engine::scene::SceneDescription::from_ron(&source).unwrap();
    let r = f.report("assets/a.bin");
    assert!(r.is_complete(), "{:?}", r.diagnostics);
    assert_eq!(r.edges.len(), 3);
    assert_eq!(r.used_by().count(), 2);
    assert_eq!(r.edges[0].kind, ReferenceKind::Mesh);
    assert!(
        matches!(&r.edges[0].locator, SourceLocator::SceneObject { index:0, name } if name == "one")
    );
    assert_eq!(
        fs::read_to_string(f.0.join("scenes/a.ron")).unwrap(),
        source
    );
    assert_eq!(
        aether_engine::scene::SceneDescription::from_ron(&source).unwrap(),
        parsed
    );
}

#[test]
fn every_material_texture_has_a_distinct_slot_including_transparency() {
    let f = Fixture::new();
    f.write("assets/t.png", b"image endpoint");
    f.write("assets/m.ron", r#"(albedo_texture:Some("t.png"),normal_texture:Some("t.png"),orm_texture:Some("t.png"),emissive_texture:Some("t.png"),transparent:Some((texture:Some("t.png"))))"#);
    let r = f.report("assets/m.ron");
    assert!(r.is_complete(), "{:?}", r.diagnostics);
    assert_eq!(r.depends_on().count(), 5);
    let kinds: std::collections::BTreeSet<_> = r.edges.iter().map(|e| e.kind).collect();
    assert_eq!(kinds.len(), 5);
    assert!(kinds.contains(&ReferenceKind::TransparentTexture));
    assert!(r.edges.iter().all(|e| e.candidate == "assets/t.png"
        && e.authored == "t.png"
        && e.resolution == Resolution::Resolved));
}

#[test]
fn terrain_and_water_visit_all_authored_maps() {
    let f = Fixture::new();
    f.write("assets/t.png", b"endpoint");
    f.write("scenes/a.ron", scene(r#"terrain:Some((source:Heightmap("assets/t.png"),splatmap:Some("assets/t.png"),layers:[(albedo_texture:Some("assets/t.png"),normal_texture:Some("assets/t.png"),roughness_metallic_texture:Some("assets/t.png"))])),water:Some((dudv_map:Some("assets/t.png"),normal_map:Some("assets/t.png")))"#));
    let r = f.report("scenes/a.ron");
    assert!(r.is_complete(), "{:?}", r.diagnostics);
    assert_eq!(r.edges.len(), 7);
    assert_eq!(
        r.edges
            .iter()
            .map(|e| e.kind)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        7
    );
}

#[test]
fn nested_prefab_and_scene_instance_overrides_keep_stable_locators() {
    let f = Fixture::new();
    f.write("assets/t.png", b"endpoint");
    f.write("assets/p.ron", r#"(schema_version:2,root:(instance_id:1,name:"root",components:[Transform(translation:(0.0,0.0,0.0),rotation_xyzw:(0.0,0.0,0.0,1.0),scale:(1.0,1.0,1.0))],children:[(instance_id:2,name:"child",components:[Transform(translation:(0.0,0.0,0.0),rotation_xyzw:(0.0,0.0,0.0,1.0),scale:(1.0,1.0,1.0)),Mesh(source:File("assets/t.png")),Material(config:(normal_texture:Some("assets/t.png")))])]))"#);
    f.write("scenes/a.ron", scene(r#"prefab_instances:[(instance_id:42,prefab_asset:"assets/p.ron",overrides:(patches:[MeshSource(instance_id:2,source:File("assets/t.png")),MaterialConfig(instance_id:2,config:(transparent:Some((texture:Some("assets/t.png"))))),NameValue(instance_id:2,value:"assets/ignore.png")]))]"#));
    let r = f.report("assets/t.png");
    assert!(r.is_complete(), "{:?}", r.diagnostics);
    assert_eq!(r.edges.len(), 5);
    assert!(r
        .edges
        .iter()
        .any(|e| matches!(e.locator, SourceLocator::PrefabNode { instance_id: 2, .. })));
    assert!(r.edges.iter().any(|e| matches!(
        e.locator,
        SourceLocator::ScenePrefab {
            instance_id: 42,
            override_index: Some(1),
            node_id: Some(2),
            ..
        }
    )));
}

#[test]
fn material_parent_resolution_preserves_spelling_and_rejects_escape() {
    let f = Fixture::new();
    f.write("assets/t.png", b"endpoint");
    f.write(
        "assets/materials/m.ron",
        r#"(normal_texture:Some("../t.png"),orm_texture:Some("../../../escape.png"))"#,
    );
    let r = f.report("assets/materials/m.ron");
    assert!(!r.is_complete());
    assert!(r.edges.iter().any(|e| e.authored == "../t.png"
        && e.candidate == "assets/t.png"
        && e.resolution == Resolution::Resolved));
    assert!(r
        .edges
        .iter()
        .any(|e| e.authored == "../../../escape.png"
            && matches!(e.resolution, Resolution::Unsafe(_))));
    assert!(!r.diagnostics.is_empty());
}

#[test]
fn missing_malformed_unknown_and_unsupported_documents_block_completeness() {
    let f = Fixture::new();
    f.write("assets/m.ron", r#"(normal_texture:Some("missing.png"))"#);
    f.write(
        "assets/unknown.ron",
        "(future_reference:Some(\"assets/x.png\"))",
    );
    f.write("scenes/bad.ron", "(name:");
    f.write("assets/future.ron", "(schema_version:99,root:())");
    let r = f.report("assets/m.ron");
    assert!(!r.is_complete());
    assert_eq!(r.edges.len(), 1);
    assert_eq!(r.edges[0].resolution, Resolution::Missing);
    for path in ["assets/unknown.ron", "scenes/bad.ron", "assets/future.ron"] {
        assert!(
            r.diagnostics.iter().any(|d| d.path == path),
            "missing diagnostic for {path}"
        );
    }
}

#[test]
fn scalar_and_transparent_only_materials_and_v1_prefabs_are_supported() {
    let f = Fixture::new();
    f.write("assets/scalar.ron", "(roughness:0.5)");
    f.write("assets/t.png", b"endpoint");
    f.write(
        "assets/glass.ron",
        r#"(transparent:Some((texture:Some("t.png"))))"#,
    );
    f.write("assets/v1.ron", r#"(schema_version:1,root:(instance_id:1,name:"old",transform:Some(()),components:[Mesh(source:File("assets/t.png"))]))"#);
    let r = f.report("assets/scalar.ron");
    assert!(r.is_complete(), "{:?}", r.diagnostics);
    assert_eq!(r.edges.len(), 2);
}

#[test]
fn deterministic_graph_and_fingerprints_leave_all_bytes_unchanged() {
    let a = Fixture::new();
    let b = Fixture::new();
    let entries = [
        ("assets/z.ron", "(normal_texture:Some(\"t.png\"))"),
        ("assets/t.png", "bytes"),
        (
            "scenes/a.ron",
            "(name:\"a\",camera:(position:(0.0,0.0,0.0)))",
        ),
    ];
    for (p, s) in entries {
        a.write(p, s);
    }
    for (p, s) in entries.into_iter().rev() {
        b.write(p, s);
    }
    let ar = a.report("assets/t.png");
    let br = b.report("assets/t.png");
    assert_eq!(ar, br);
    for (p, s) in entries {
        assert_eq!(fs::read(a.0.join(p)).unwrap(), s.as_bytes());
    }
    assert!(!ar.is_stale(&a.0, AnalysisBudget::default()).unwrap());
}

#[test]
fn full_bytes_target_replacement_and_inventory_change_fingerprints() {
    let f = Fixture::new();
    f.write("assets/t.bin", "abcd");
    f.write("assets/m.ron", "(roughness:0.5)");
    let first = f.report("assets/t.bin");
    f.write("assets/t.bin", "abce");
    let second = f.report("assets/t.bin");
    assert_ne!(first.files["assets/t.bin"], second.files["assets/t.bin"]);
    assert!(first.is_stale(&f.0, AnalysisBudget::default()).unwrap());
    f.write("assets/m.ron", "(roughness:0.6)");
    let third = f.report("assets/t.bin");
    assert_ne!(second.files["assets/m.ron"], third.files["assets/m.ron"]);
    fs::create_dir(f.0.join("assets/empty")).unwrap();
    let fourth = f.report("assets/t.bin");
    assert_ne!(third.fingerprint, fourth.fingerprint);
    fs::remove_dir(f.0.join("assets/empty")).unwrap();
    assert_eq!(third.fingerprint, f.report("assets/t.bin").fingerprint);
    f.write("assets/new.bin", "new");
    assert_ne!(third.fingerprint, f.report("assets/t.bin").fingerprint);
    fs::remove_file(f.0.join("assets/new.bin")).unwrap();
    assert_eq!(third.fingerprint, f.report("assets/t.bin").fingerprint);
    fs::remove_file(f.0.join("assets/t.bin")).unwrap();
    let last = f.report("assets/t.bin");
    assert_ne!(third.fingerprint, last.fingerprint);
    assert!(!last.is_complete());
}

#[test]
fn endpoints_do_not_block_but_unvisited_authored_formats_do() {
    let f = Fixture::new();
    for path in ["assets/a.png", "assets/b.bin", "assets/c.jpg"] {
        f.write(path, "payload");
    }
    assert!(f.report("assets/a.png").is_complete());
    for path in ["assets/a.obj", "assets/a.mtl", "assets/a.gltf"] {
        f.write(path, "payload");
    }
    let r = f.report("assets/a.png");
    assert!(!r.is_complete());
    assert_eq!(r.diagnostics.len(), 3);
}

#[test]
fn explicit_count_and_stream_byte_budgets_block_incomplete_scans() {
    let f = Fixture::new();
    f.write("assets/a.bin", "0123456789");
    for budget in [
        AnalysisBudget {
            max_files: 0,
            ..AnalysisBudget::default()
        },
        AnalysisBudget {
            max_file_bytes: 5,
            ..AnalysisBudget::default()
        },
        AnalysisBudget {
            max_total_bytes: 5,
            ..AnalysisBudget::default()
        },
        AnalysisBudget {
            max_entries: 0,
            ..AnalysisBudget::default()
        },
    ] {
        let r = analyze(&f.0, "assets/a.bin", budget).unwrap();
        assert!(!r.is_complete());
        assert!(
            r.diagnostics.iter().any(|d| d.message.contains("budget")),
            "{:?}",
            r.diagnostics
        );
    }
}

#[cfg(unix)]
#[test]
fn descendant_and_reference_symlinks_are_blockers_even_when_index_skips_them() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    f.write("assets/t.png", "payload");
    symlink(f.0.join("assets/t.png"), f.0.join("assets/link.png")).unwrap();
    symlink(f.0.join("assets"), f.0.join("assets/linked-directory")).unwrap();
    f.write("assets/m.ron", r#"(normal_texture:Some("link.png"))"#);
    let index = aether_launcher::project_index::scan_project(&f.0).unwrap();
    assert!(index.entries.iter().all(|e| !e.path.contains("link")));
    let r = f.report("assets/t.png");
    assert!(!r.is_complete());
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.path == "assets/linked-directory"));
    assert!(matches!(r.edges[0].resolution, Resolution::Unsafe(_)));
    assert_eq!(r.edges[0].authored, "link.png");
    assert_eq!(r.edges[0].candidate, "assets/link.png");
}

#[test]
fn unknown_nested_reference_fields_block_supported_schemas() {
    let f = Fixture::new();
    f.write(
        "scenes/a.ron",
        scene(r#"objects:[(mesh:Builtin("cube"),material:(future_texture:Some("assets/x.png")))]"#),
    );
    let r = f.report("scenes/a.ron");
    assert!(!r.is_complete());
    assert!(r.diagnostics.iter().any(|d| d.path == "scenes/a.ron"));
}

#[test]
fn direct_asset_references_use_asset_manager_slash_normalization() {
    let f = Fixture::new();
    f.write("assets/t.bin", "endpoint");
    f.write(
        "scenes/a.ron",
        scene(r#"objects:[(mesh:File("assets\\t.bin"))]"#),
    );
    let r = f.report("assets/t.bin");
    assert!(r.is_complete(), "{:?}", r.diagnostics);
    assert_eq!(r.edges[0].candidate, "assets/t.bin");
    assert_eq!(r.edges[0].authored, r"assets\t.bin");
}

#[cfg(unix)]
#[test]
fn conflicting_texture_lookup_and_asset_identity_block_completeness() {
    let f = Fixture::new();
    f.write(r"assets/folder\t.png", "endpoint");
    f.write("assets/m.ron", r#"(normal_texture:Some("folder\\t.png"))"#);
    let r = f.report("assets/m.ron");
    assert!(!r.is_complete());
    assert!(matches!(r.edges[0].resolution, Resolution::Unsafe(_)));
}

#[test]
fn allowlisted_roots_must_be_directories() {
    let f = Fixture::new();
    fs::remove_dir(f.0.join("assets")).unwrap();
    fs::write(f.0.join("assets"), "not a directory").unwrap();
    f.write("scenes/a.ron", scene(""));
    let r = f.report("scenes/a.ron");
    assert!(!r.is_complete());
    assert!(r.diagnostics.iter().any(|d| d.path == "assets"));
}

#[test]
fn self_contained_gltf_is_endpoint_but_external_buffers_block() {
    let f = Fixture::new();
    f.write("assets/target.bin", b"data");
    f.write("assets/mesh.gltf", r#"{"asset":{"version":"2.0"}}"#);
    assert!(f.report("assets/target.bin").is_complete());
    f.write(
        "assets/mesh.gltf",
        r#"{"asset":{"version":"2.0"},"buffers":[{"uri":"target.bin","byteLength":4}]}"#,
    );
    let r = f.report("assets/target.bin");
    assert!(!r.is_complete());
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.path == "assets/mesh.gltf" && d.message.contains("external-resource")));
}

#[test]
fn external_gltf_images_block_even_without_external_buffers() {
    let f = Fixture::new();
    f.write("assets/t.png", b"image endpoint");
    f.write(
        "assets/mesh.gltf",
        r#"{"asset":{"version":"2.0"},"images":[{"uri":"t.png"}]}"#,
    );
    let r = f.report("assets/t.png");
    assert!(!r.is_complete());
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.path == "assets/mesh.gltf" && d.message.contains("external-resource")));
}

#[test]
fn terrain_and_water_follow_asset_manager_slash_normalization() {
    let f = Fixture::new();
    f.write("assets/t.png", b"endpoint");
    f.write("scenes/a.ron", scene(r#"terrain:Some((source:Heightmap("assets\\t.png"),splatmap:Some("assets\\t.png"),layers:[(albedo_texture:Some("assets\\t.png"),normal_texture:Some("assets\\t.png"),roughness_metallic_texture:Some("assets\\t.png"))])),water:Some((dudv_map:Some("assets\\t.png"),normal_map:Some("assets\\t.png")))"#));
    let r = f.report("scenes/a.ron");
    assert!(r.is_complete(), "{:?}", r.diagnostics);
    assert_eq!(r.edges.len(), 7);
    assert!(r.edges.iter().all(|e| e.candidate == "assets/t.png"));
}

#[test]
fn optional_gltf_extensions_with_external_uris_block_completeness() {
    let f = Fixture::new();
    f.write("assets/payload.bin", b"external resource");
    let source = r#"{
        "asset":{"version":"2.0"},
        "extensionsUsed":["VENDOR_external_resource"],
        "extensions":{"VENDOR_external_resource":{"uri":"payload.bin"}}
    }"#;
    let parsed = gltf::Gltf::from_slice(source.as_bytes()).unwrap();
    assert_eq!(parsed.extensions_required().count(), 0);
    assert_eq!(parsed.extensions_used().count(), 1);
    f.write("assets/mesh.gltf", source);

    let report = f.report("assets/payload.bin");
    assert!(!report.is_complete());
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.path == "assets/mesh.gltf"
            && diagnostic.message.contains("VENDOR_external_resource")
            && diagnostic.message.contains("extension")
    }));
}

#[path = "dependency_analysis/extensions.rs"]
mod extensions;
