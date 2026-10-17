use super::Fixture;

const ROOT_EXTENSION: &str = r#"{
    "asset":{"version":"2.0"},
    "extensions":{"VENDOR_external_resource":{"uri":"payload.bin"}}
}"#;

fn glb(source: &str) -> Vec<u8> {
    let mut json = source.as_bytes().to_vec();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    let mut bytes = b"glTF".to_vec();
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&(20 + json.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(json.len() as u32).to_le_bytes());
    bytes.extend_from_slice(b"JSON");
    bytes.extend_from_slice(&json);
    bytes
}

fn assert_undeclared_extension_blocks(bytes: &[u8], extension: &str) {
    let parsed = gltf::Gltf::from_slice(bytes).unwrap();
    assert_eq!(parsed.extensions_used().count(), 0);
    assert_eq!(parsed.extensions_required().count(), 0);
    let fixture = Fixture::new();
    fixture.write("assets/payload.bin", b"external resource");
    let path = format!("assets/mesh.{extension}");
    fixture.write(&path, bytes);

    let report = fixture.report("assets/payload.bin");
    assert!(!report.is_complete());
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.path == path
            && diagnostic.message.contains("VENDOR_external_resource")
            && diagnostic.message.contains("extension")
    }));
}

#[test]
fn undeclared_root_extension_payload_blocks_completeness() {
    assert_undeclared_extension_blocks(ROOT_EXTENSION.as_bytes(), "gltf");
}

#[test]
fn undeclared_nested_extension_payload_blocks_completeness() {
    let source = r#"{
        "asset":{"version":"2.0"},
        "nodes":[{"extensions":{"VENDOR_external_resource":{"uri":"payload.bin"}}}]
    }"#;
    assert_undeclared_extension_blocks(source.as_bytes(), "gltf");
}

#[test]
fn undeclared_glb_extension_payload_blocks_completeness() {
    assert_undeclared_extension_blocks(&glb(ROOT_EXTENSION), "glb");
}
