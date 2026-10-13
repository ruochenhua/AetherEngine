use crate::asset::{texture::CpuTexture, AssetManager};
use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};

#[test]
fn immutable_snapshots_preserve_source_cache_and_previous_pixel_versions() {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "aether-texture-snapshot-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("color.png");
    image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]))
        .save(&path)
        .unwrap();
    let mut assets = AssetManager::with_project_root(&root);
    let source = assets.load::<CpuTexture>("color.png").unwrap();
    let red = assets
        .register_texture_snapshot(CpuTexture::from_file(&path).unwrap())
        .unwrap();
    image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 255, 255]))
        .save(&path)
        .unwrap();
    let blue = assets
        .register_texture_snapshot(CpuTexture::from_file(&path).unwrap())
        .unwrap();
    assert_ne!(
        assets.asset_id(red).unwrap(),
        assets.asset_id(blue).unwrap()
    );
    assert_ne!(
        red.id(),
        blue.id(),
        "GPU cache keys must distinguish pixel versions"
    );
    assert_eq!(assets.get(blue).unwrap().data, [0, 0, 255, 255]);
    assert_eq!(assets.get(red).unwrap().data, [255, 0, 0, 255]);
    assert_eq!(assets.get(source).unwrap().data, [255, 0, 0, 255]);
    assert_eq!(
        assets.load::<CpuTexture>("color.png").unwrap(),
        source,
        "ordinary load keeps its existing cache-hit behavior"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn snapshot_identity_deduplicates_pixels_and_distinguishes_dimensions() {
    let mut assets = AssetManager::new();
    let first = assets
        .register_texture_snapshot(CpuTexture::from_color(255, 0, 0, 255))
        .unwrap();
    let repeated = assets
        .register_texture_snapshot(CpuTexture::from_color(255, 0, 0, 255))
        .unwrap();
    assert_eq!(first, repeated);
    let mut wide = CpuTexture::from_color(255, 0, 0, 255);
    wide.width = 2;
    wide.data.extend([255, 0, 0, 255]);
    let mut tall = wide.clone();
    tall.width = 1;
    tall.height = 2;
    let wide = assets.register_texture_snapshot(wide).unwrap();
    let tall = assets.register_texture_snapshot(tall).unwrap();
    assert_ne!(wide, tall);
    assert_eq!(assets.get(wide).unwrap().width, 2);
    assert_eq!(assets.get(tall).unwrap().height, 2);
}

#[test]
fn snapshot_registration_rejects_shutdown_without_invalidating_old_handles() {
    let mut assets = AssetManager::new();
    let old = assets
        .register_texture_snapshot(CpuTexture::from_color(255, 0, 0, 255))
        .unwrap();
    assets.shutdown().unwrap();
    assert!(assets
        .register_texture_snapshot(CpuTexture::from_color(0, 0, 255, 255))
        .is_err());
    assert_eq!(assets.get(old).unwrap().data, [255, 0, 0, 255]);
}
