use super::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "aether-files-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::create_dir(path.join("assets")).unwrap();
        fs::create_dir(path.join("scenes")).unwrap();
        Self(path)
    }
    fn source(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }
    fn empty_assets(&self) {
        assert_eq!(fs::read_dir(self.0.join("assets")).unwrap().count(), 0);
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn image_bytes(format: image::ImageFormat) -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::new(2, 2))
        .write_to(&mut bytes, format)
        .unwrap();
    bytes.into_inner()
}
fn glb(json: serde_json::Value, bin: Option<&[u8]>) -> Vec<u8> {
    let mut json = serde_json::to_vec(&json).unwrap();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    let mut bytes = Vec::from(&b"glTF\x02\0\0\0\0\0\0\0"[..]);
    bytes.extend((json.len() as u32).to_le_bytes());
    bytes.extend(b"JSON");
    bytes.extend(json);
    if let Some(bin) = bin {
        bytes.extend((bin.len() as u32).to_le_bytes());
        bytes.extend(b"BIN\0");
        bytes.extend(bin);
    }
    let len = bytes.len() as u32;
    bytes[8..12].copy_from_slice(&len.to_le_bytes());
    bytes
}
fn mesh_json() -> serde_json::Value {
    serde_json::json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteLength":36}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0})
}
fn triangle() -> Vec<u8> {
    [0f32, 0., 0., 1., 0., 0., 0., 1., 0.]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect()
}

#[test]
fn creates_folders_under_both_content_roots() {
    let p = Project::new();
    for root in ["assets", "scenes"] {
        assert_eq!(
            create_folder(&p.0, root, "Models").unwrap(),
            format!("{root}/Models")
        );
        assert!(p.0.join(root).join("Models").is_dir());
    }
}
#[test]
fn rejects_invalid_names_without_mutation() {
    let p = Project::new();
    for name in [
        "",
        " ",
        ".",
        "..",
        ".hidden",
        "a/b",
        "a\\b",
        "a:b",
        "a?b",
        "a*b",
        "a\nb",
        "a<b",
        "a>b",
        "a|b",
        "a\"b",
        "trailing.",
        "trailing ",
        "CON",
        "nul.txt",
    ] {
        assert!(
            matches!(
                create_folder(&p.0, "assets", name),
                Err(FileOperationError::Path(_))
            ),
            "{name:?}"
        );
    }
    p.empty_assets();
}
#[test]
fn rejects_folder_collisions_and_unsafe_destinations() {
    let p = Project::new();
    fs::create_dir(p.0.join("assets/Models")).unwrap();
    assert!(matches!(
        create_folder(&p.0, "assets", "models"),
        Err(FileOperationError::Collision(_))
    ));
    for directory in [
        "",
        ".",
        "../assets",
        "assets/../scenes",
        "/assets",
        "other",
        "assets/missing",
        "assets\\Models",
    ] {
        assert!(
            matches!(
                create_folder(&p.0, directory, "Nope"),
                Err(FileOperationError::Path(_))
            ),
            "{directory}"
        );
    }
    fs::write(p.0.join("assets/file"), b"a").unwrap();
    assert!(matches!(
        create_folder(&p.0, "assets/file", "Nope"),
        Err(FileOperationError::Path(_))
    ));
}
#[test]
fn imports_real_images_preserving_bytes_and_existing_files() {
    let p = Project::new();
    for (name, format) in [
        ("a.png", image::ImageFormat::Png),
        ("b.jpg", image::ImageFormat::Jpeg),
        ("c.JPEG", image::ImageFormat::Jpeg),
    ] {
        let bytes = image_bytes(format);
        let source = p.source(name, &bytes);
        assert_eq!(
            import_file(&p.0, "assets", &source).unwrap(),
            format!("assets/{name}")
        );
        assert_eq!(fs::read(p.0.join("assets").join(name)).unwrap(), bytes);
        assert!(matches!(
            import_file(&p.0, "assets", &source),
            Err(FileOperationError::Collision(_))
        ));
        assert_eq!(fs::read(p.0.join("assets").join(name)).unwrap(), bytes);
    }
    assert_eq!(fs::read_dir(p.0.join("assets")).unwrap().count(), 3);
}
#[test]
fn rejects_corrupt_mismatched_and_unsupported_images_without_staging() {
    let p = Project::new();
    let corrupt = p.source("bad.png", b"invalid");
    assert!(matches!(
        import_file(&p.0, "assets", &corrupt),
        Err(FileOperationError::Validation(_))
    ));
    let mismatch = p.source("wrong.jpg", &image_bytes(image::ImageFormat::Png));
    assert!(matches!(
        import_file(&p.0, "assets", &mismatch),
        Err(FileOperationError::Validation(_))
    ));
    let unsupported = p.source("bad.obj", b"v 0 0 0");
    assert!(matches!(
        import_file(&p.0, "assets", &unsupported),
        Err(FileOperationError::UnsupportedFormat(_))
    ));
    p.empty_assets();
}
#[test]
fn imports_embedded_glb_mesh() {
    let p = Project::new();
    let bytes = glb(mesh_json(), Some(&triangle()));
    let source = p.source("mesh.glb", &bytes);
    assert_eq!(
        import_file(&p.0, "assets", &source).unwrap(),
        "assets/mesh.glb"
    );
    assert_eq!(fs::read(p.0.join("assets/mesh.glb")).unwrap(), bytes);
}
#[test]
fn rejects_external_buffer_and_image_dependencies() {
    let p = Project::new();
    for uri in ["mesh.bin", "../mesh.bin", "https://example.com/mesh.bin"] {
        let mut json = mesh_json();
        json["buffers"][0]["uri"] = uri.into();
        let source = p.source("external.glb", &glb(json, None));
        match import_file(&p.0, "assets", &source) {
            Err(FileOperationError::ExternalDependency(message)) => assert!(message.contains(uri)),
            other => panic!("{other:?}"),
        }
    }
    let mut json = mesh_json();
    json["images"] = serde_json::json!([{"uri":"texture.png"}]);
    let source = p.source("external.glb", &glb(json, Some(&triangle())));
    assert!(matches!(
        import_file(&p.0, "assets", &source),
        Err(FileOperationError::ExternalDependency(_))
    ));
    p.empty_assets();
}
#[test]
fn rejects_invalid_glb_container_and_engine_mesh() {
    let p = Project::new();
    let valid = glb(mesh_json(), Some(&triangle()));
    let mut bad_length = valid.clone();
    bad_length[8] = 0;
    let mut bad_chunk = valid.clone();
    bad_chunk[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut bad_version = valid;
    bad_version[4] = 1;
    let no_mesh = glb(serde_json::json!({"asset":{"version":"2.0"}}), None);
    for bytes in [
        b"{\"asset\":{\"version\":\"2.0\"}}".to_vec(),
        bad_length,
        bad_chunk,
        bad_version,
        no_mesh,
    ] {
        let source = p.source("bad.glb", &bytes);
        assert!(matches!(
            import_file(&p.0, "assets", &source),
            Err(FileOperationError::Validation(_))
        ));
        p.empty_assets();
    }
}
#[test]
fn import_rejects_unsafe_destination_and_case_collisions() {
    let p = Project::new();
    let source = p.source("test.png", &image_bytes(image::ImageFormat::Png));
    for directory in [
        "scenes",
        "assets/..",
        "../assets",
        "/assets",
        "other",
        "assets/missing",
    ] {
        assert!(matches!(
            import_file(&p.0, directory, &source),
            Err(FileOperationError::Path(_))
        ));
        p.empty_assets();
    }
    fs::write(p.0.join("assets/TEST.PNG"), b"original").unwrap();
    assert!(matches!(
        import_file(&p.0, "assets", &source),
        Err(FileOperationError::Collision(_))
    ));
    assert_eq!(fs::read(p.0.join("assets/TEST.PNG")).unwrap(), b"original");
    assert_eq!(fs::read_dir(p.0.join("assets")).unwrap().count(), 1);
}
#[cfg(unix)]
#[test]
fn rejects_symlinks_in_root_destination_and_source() {
    use std::os::unix::fs::symlink;
    let p = Project::new();
    let source = p.source("test.png", &image_bytes(image::ImageFormat::Png));
    symlink(p.0.join("scenes"), p.0.join("assets/link")).unwrap();
    assert!(matches!(
        create_folder(&p.0, "assets/link", "Nope"),
        Err(FileOperationError::Path(_))
    ));
    assert!(matches!(
        import_file(&p.0, "assets/link", &source),
        Err(FileOperationError::Path(_))
    ));
    let link = p.0.join("source.png");
    symlink(&source, &link).unwrap();
    assert!(matches!(
        import_file(&p.0, "assets", &link),
        Err(FileOperationError::Path(_))
    ));
    let root_link = p.0.with_extension("link");
    symlink(&p.0, &root_link).unwrap();
    assert!(matches!(
        create_folder(&root_link, "assets", "Nope"),
        Err(FileOperationError::Path(_))
    ));
    fs::remove_file(root_link).unwrap();
    fs::remove_dir(p.0.join("scenes")).unwrap();
    symlink(p.0.join("assets"), p.0.join("scenes")).unwrap();
    assert!(matches!(
        create_folder(&p.0, "scenes", "Nope"),
        Err(FileOperationError::Path(_))
    ));
    assert!(!p.0.join("scenes/Nope").exists());
}
