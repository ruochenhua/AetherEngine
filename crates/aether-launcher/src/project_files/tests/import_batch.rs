use super::*;

#[test]
fn failed_folder_package_rolls_back_while_independent_batch_items_can_import() {
    let p = Project::new();
    let source_root = p.0.join("external/Pack/nested");
    fs::create_dir_all(&source_root).unwrap();
    fs::write(
        p.0.join("external/Pack/nested/valid.png"),
        image_bytes(image::ImageFormat::Png),
    )
    .unwrap();
    fs::write(
        p.0.join("external/Pack/readme.txt"),
        b"unsupported package item",
    )
    .unwrap();
    let standalone = p.source("standalone.png", &image_bytes(image::ImageFormat::Png));

    let report = import_paths(&p.0, "assets", &[standalone, p.0.join("external/Pack")]);

    assert_eq!(report.imported, ["assets/standalone.png"]);
    assert_eq!(report.failures.len(), 1);
    assert!(!p.0.join("assets/Pack").exists());
    assert_eq!(fs::read_dir(p.0.join("assets")).unwrap().count(), 1);
}

#[test]
fn folder_package_rejects_missing_obj_material_dependency_without_publishing() {
    let p = Project::new();
    let package = p.0.join("external/Pack");
    fs::create_dir_all(&package).unwrap();
    fs::write(
        package.join("mesh.obj"),
        "mtllib missing.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
    )
    .unwrap();

    let report = import_paths(&p.0, "assets", &[package]);

    assert!(report.imported.is_empty());
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].contains("missing.mtl"));
    assert!(!p.0.join("assets/Pack").exists());
    p.empty_assets();
}

#[test]
fn imports_obj_package_with_nested_material_texture_dependencies() {
    let p = Project::new();
    let package = p.0.join("external/Models");
    fs::create_dir_all(package.join("Textures")).unwrap();
    fs::write(
        package.join("mesh.obj"),
        "mtllib mesh.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
    )
    .unwrap();
    fs::write(
        package.join("mesh.mtl"),
        "newmtl default\nmap_Kd Textures/checker.png\n",
    )
    .unwrap();
    fs::write(
        package.join("Textures/checker.png"),
        image_bytes(image::ImageFormat::Png),
    )
    .unwrap();

    let report = import_paths(&p.0, "assets", &[package]);

    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(
        report.imported,
        [
            "assets/Models/Textures/checker.png",
            "assets/Models/mesh.mtl",
            "assets/Models/mesh.obj"
        ]
    );
    assert!(p.0.join("assets/Models/Textures/checker.png").is_file());
}

#[test]
fn folder_package_rejects_traversal_obj_dependency_without_publishing() {
    let p = Project::new();
    let package = p.0.join("external/Models");
    fs::create_dir_all(&package).unwrap();
    fs::write(p.0.join("external/outside.mtl"), "newmtl outside\n").unwrap();
    fs::write(
        package.join("mesh.obj"),
        "mtllib ../outside.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
    )
    .unwrap();

    let report = import_paths(&p.0, "assets", &[package]);

    assert!(report.imported.is_empty());
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].contains("outside.mtl"));
    assert!(!p.0.join("assets/Models").exists());
    p.empty_assets();
}

#[cfg(unix)]
#[test]
fn folder_package_with_nested_symlink_is_rejected_without_partial_publish() {
    use std::os::unix::fs::symlink;

    let p = Project::new();
    let package = p.0.join("external/Pack");
    fs::create_dir_all(&package).unwrap();
    fs::write(
        package.join("valid.png"),
        image_bytes(image::ImageFormat::Png),
    )
    .unwrap();
    let outside = p.source("outside.png", &image_bytes(image::ImageFormat::Png));
    symlink(outside, package.join("linked.png")).unwrap();

    let report = import_paths(&p.0, "assets", &[package]);

    assert!(report.imported.is_empty());
    assert_eq!(report.failures.len(), 1);
    assert!(!p.0.join("assets/Pack").exists());
    p.empty_assets();
}
