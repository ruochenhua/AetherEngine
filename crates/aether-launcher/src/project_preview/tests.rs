use super::*;
use std::path::PathBuf;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "aether-preview-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("assets")).unwrap();
        Self(root)
    }
    fn write(&self, path: &str, data: impl AsRef<[u8]>) {
        std::fs::write(self.0.join(path), data).unwrap();
    }
    fn load(&self, path: &str, kind: ResourceKind) -> Result<Thumbnail, String> {
        load(&self.0, path, kind, Limits::default())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn projection_centers_negative_asymmetric_bounds_and_is_deterministic() {
    let positions = [[-8., -3., -4.], [-2., -3., -4.], [-8., 1., -2.]];
    let result = mesh::project(&positions, &[[0, 1, 2]], Limits::default()).unwrap();
    assert_eq!(result.dimensions, "AABB: 6.000 × 4.000 × 2.000");
    assert_eq!(result.size, [384, 384]);
    assert_eq!(result.rgba.len(), 384 * 384 * 4);
    assert!(result
        .rgba
        .chunks_exact(4)
        .any(|pixel| pixel != [28, 32, 38, 255]));
    assert_eq!(
        result,
        mesh::project(&positions, &[[0, 1, 2]], Limits::default()).unwrap()
    );
}

#[test]
fn projection_rejects_empty_nonfinite_bad_indices_and_limits() {
    let limits = Limits {
        vertices: 3,
        triangles: 1,
        ..Default::default()
    };
    let positions = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
    for (vertices, indices, reason) in [
        (vec![], vec![], "empty"),
        (vec![[f32::NAN, 0., 0.]], vec![[0, 0, 0]], "finite"),
        (positions.to_vec(), vec![[0, 1, 3]], "index"),
        (positions.to_vec(), vec![[0, 1, 2]; 2], "triangle"),
        (vec![[0., 0., 0.]; 4], vec![[0, 1, 2]], "vertex"),
    ] {
        assert!(mesh::project(&vertices, &indices, limits)
            .unwrap_err()
            .contains(reason));
    }
}

#[test]
fn image_reports_original_dimensions_and_fits_thumbnail() {
    let fixture = Fixture::new();
    image::RgbaImage::from_pixel(800, 200, image::Rgba([30, 80, 160, 255]))
        .save(fixture.0.join("assets/wide.png"))
        .unwrap();
    let result = fixture
        .load("assets/wide.png", ResourceKind::Image)
        .unwrap();
    assert_eq!(result.dimensions, "Image: 800 × 200 px");
    assert_eq!(result.size, [384, 96]);
    assert_eq!(result.rgba.len(), 384 * 96 * 4);
    assert_eq!(&result.rgba[..4], &[30, 80, 160, 255]);
}

#[test]
fn rejects_unreadable_malformed_and_oversized_images() {
    let fixture = Fixture::new();
    fixture.write("assets/bad.png", b"not an image");
    assert!(fixture
        .load("assets/missing.png", ResourceKind::Image)
        .is_err());
    assert!(fixture.load("assets/bad.png", ResourceKind::Image).is_err());
    image::RgbaImage::new(10, 10)
        .save(fixture.0.join("assets/test.png"))
        .unwrap();
    for limits in [
        Limits {
            file_bytes: 1,
            ..Default::default()
        },
        Limits {
            image_edge: 9,
            ..Default::default()
        },
        Limits {
            image_pixels: 99,
            ..Default::default()
        },
    ] {
        assert!(
            load(&fixture.0, "assets/test.png", ResourceKind::Image, limits)
                .unwrap_err()
                .contains("limit")
        );
    }
}

const OBJ: &str = "v -8 -3 -4\nv -2 -3 -4\nv -8 1 -2\nf 1 2 3\n";

#[test]
fn safe_obj_supports_bounds_and_negative_face_indices() {
    let fixture = Fixture::new();
    fixture.write("assets/mesh.obj", OBJ.replace("f 1 2 3", "f -3 -2 -1"));
    assert_eq!(
        fixture
            .load("assets/mesh.obj", ResourceKind::Obj)
            .unwrap()
            .dimensions,
        "AABB: 6.000 × 4.000 × 2.000"
    );
    fixture.write("assets/material.mtl", "newmtl neutral");
    fixture.write("assets/mesh.obj", format!("mtllib material.mtl\n{OBJ}"));
    assert!(fixture.load("assets/mesh.obj", ResourceKind::Obj).is_ok());
}

#[test]
fn obj_rejects_escaping_dependencies_malformed_geometry_and_limits() {
    let fixture = Fixture::new();
    for source in [
        format!("mtllib ../../outside.mtl\n{OBJ}"),
        OBJ.replace("f 1 2 3", "f 1 2 9"),
        OBJ.replace("v -8", "v NaN"),
    ] {
        fixture.write("assets/mesh.obj", source);
        assert!(fixture.load("assets/mesh.obj", ResourceKind::Obj).is_err());
    }
    fixture.write("assets/mesh.obj", OBJ);
    assert!(load(
        &fixture.0,
        "assets/mesh.obj",
        ResourceKind::Obj,
        Limits {
            vertices: 2,
            ..Default::default()
        }
    )
    .unwrap_err()
    .contains("vertex"));
}

fn gltf(uri: &str) -> String {
    serde_json::json!({"asset":{"version":"2.0"},"buffers":[{"uri":uri,"byteLength":36}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[2,1,0]}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0}).to_string()
}

#[test]
fn gltf_loads_contained_and_embedded_buffers() {
    let fixture = Fixture::new();
    let bytes: Vec<u8> = [0_f32, 0., 0., 2., 0., 0., 0., 1., 0.]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    fixture.write("assets/mesh.bin", bytes);
    fixture.write("assets/mesh.gltf", gltf("mesh.bin"));
    assert_eq!(
        fixture
            .load("assets/mesh.gltf", ResourceKind::Gltf)
            .unwrap()
            .dimensions,
        "AABB: 2.000 × 1.000 × 0.000"
    );
    fixture.write(
        "assets/mesh.gltf",
        gltf(
            "data:application/octet-stream;base64,AAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAgD8AAAAA",
        ),
    );
    assert!(fixture.load("assets/mesh.gltf", ResourceKind::Gltf).is_ok());
}

#[test]
fn gltf_rejects_external_buffers_and_image_dependencies() {
    let fixture = Fixture::new();
    for uri in [
        "../../outside.bin",
        "https://example.com/mesh.bin",
        "%2e%2e/out.bin",
        "/tmp/out.bin",
    ] {
        fixture.write("assets/mesh.gltf", gltf(uri));
        assert!(fixture
            .load("assets/mesh.gltf", ResourceKind::Gltf)
            .unwrap_err()
            .contains("dependency"));
    }
    let mut source: serde_json::Value = serde_json::from_str(&gltf(
        "data:application/octet-stream;base64,AAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAgD8AAAAA",
    ))
    .unwrap();
    source["images"] = serde_json::json!([{"uri":"../../outside.png"}]);
    fixture.write("assets/mesh.gltf", source.to_string());
    assert!(fixture
        .load("assets/mesh.gltf", ResourceKind::Gltf)
        .unwrap_err()
        .contains("dependency"));
}

#[cfg(unix)]
#[test]
fn rejects_symlink_selected_file_ancestors_and_dependencies() {
    let fixture = Fixture::new();
    fixture.write("assets/real.obj", OBJ);
    fixture.write("assets/real.mtl", "newmtl neutral");
    std::os::unix::fs::symlink(
        fixture.0.join("assets/real.obj"),
        fixture.0.join("assets/link.obj"),
    )
    .unwrap();
    assert!(fixture
        .load("assets/link.obj", ResourceKind::Obj)
        .unwrap_err()
        .contains("symlink"));
    std::os::unix::fs::symlink(fixture.0.join("assets"), fixture.0.join("alias")).unwrap();
    assert!(fixture
        .load("alias/real.obj", ResourceKind::Obj)
        .unwrap_err()
        .contains("symlink"));
    std::os::unix::fs::symlink(
        fixture.0.join("assets/real.mtl"),
        fixture.0.join("assets/link.mtl"),
    )
    .unwrap();
    fixture.write("assets/mesh.obj", format!("mtllib link.mtl\n{OBJ}"));
    assert!(fixture
        .load("assets/mesh.obj", ResourceKind::Obj)
        .unwrap_err()
        .contains("symlink"));
    fixture.write("assets/mesh.gltf", gltf("link.mtl"));
    assert!(fixture
        .load("assets/mesh.gltf", ResourceKind::Gltf)
        .unwrap_err()
        .contains("symlink"));
}
