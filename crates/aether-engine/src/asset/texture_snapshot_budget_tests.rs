use crate::asset::{texture::CpuTexture, AssetManager, TextureSnapshotLimits};

fn manager(bytes: usize, count: usize) -> AssetManager {
    AssetManager::with_project_root_and_texture_snapshot_limits(
        ".",
        TextureSnapshotLimits {
            decoded_bytes: bytes,
            count,
        },
    )
}

#[test]
fn snapshot_batch_budget_rejects_atomically_and_deduplicates_existing_and_batch_pixels() {
    let mut assets = manager(8, 4);
    let red = CpuTexture::from_color(255, 0, 0, 255);
    let old = assets.register_texture_snapshot(red.clone()).unwrap();
    assert_eq!(
        (
            assets.texture_snapshot_count(),
            assets.texture_snapshot_bytes()
        ),
        (1, 4)
    );
    let failed = assets.register_texture_snapshots(vec![
        CpuTexture::from_color(0, 0, 255, 255),
        CpuTexture::from_color(0, 255, 0, 255),
    ]);
    assert!(failed.err().unwrap().to_string().contains("budget"));
    assert_eq!(
        (
            assets.texture_snapshot_count(),
            assets.texture_snapshot_bytes()
        ),
        (1, 4)
    );
    assert_eq!(assets.get(old).unwrap().data, red.data);
    let repeated = assets
        .register_texture_snapshots(vec![
            red.clone(),
            red.clone(),
            CpuTexture::from_color(0, 0, 255, 255),
            red,
        ])
        .unwrap();
    assert_eq!(repeated[0], old);
    assert_eq!(repeated[1], old);
    assert_eq!(repeated[3], old);
    assert_eq!(
        (
            assets.texture_snapshot_count(),
            assets.texture_snapshot_bytes()
        ),
        (2, 8)
    );
    assert!(assets
        .register_texture_snapshot(CpuTexture::from_color(0, 255, 0, 255))
        .is_err());
    assert_eq!(assets.get(old).unwrap().data, vec![255, 0, 0, 255]);
}

#[test]
fn snapshot_count_budget_rejects_before_inserting_any_unique_content() {
    let mut assets = manager(128, 2);
    let old = assets
        .register_texture_snapshot(CpuTexture::from_color(255, 0, 0, 255))
        .unwrap();
    assert!(assets
        .register_texture_snapshots(vec![
            CpuTexture::from_color(0, 0, 255, 255),
            CpuTexture::from_color(0, 255, 0, 255)
        ])
        .is_err());
    assert_eq!(
        (
            assets.texture_snapshot_count(),
            assets.texture_snapshot_bytes()
        ),
        (1, 4)
    );
    assert!(assets.get(old).is_some());
}

#[test]
fn default_budget_accepts_valid_large_resolution_and_accounts_exact_decoded_bytes() {
    let mut assets = AssetManager::new();
    let pixels = CpuTexture {
        width: 2048,
        height: 2048,
        channels: 4,
        format: crate::asset::texture::TextureFormat::Rgba8,
        data: vec![128; 2048 * 2048 * 4],
    };
    let handle = assets.register_texture_snapshot(pixels.clone()).unwrap();
    assert_eq!(assets.texture_snapshot_bytes(), pixels.data.len());
    assert_eq!(assets.texture_snapshot_count(), 1);
    assert_eq!(assets.get(handle).unwrap().width, 2048);
    assert_eq!(
        TextureSnapshotLimits::default().decoded_bytes,
        256 * 1024 * 1024
    );
    assert_eq!(TextureSnapshotLimits::default().count, 256);
}
