use super::{material_drop::resolve_material_assignment, material_drop_tests::Fixture};
use aether_engine::{
    asset::texture::CpuTexture,
    editor::MaterialAssignment,
    scene::{
        material::{ColorSpace, TextureUsage},
        MaterialConfig, TransparentMaterialConfig,
    },
};

fn duplicate_paths_do_not_load_source_textures(linked: bool) {
    let mut f = Fixture::new();
    let mut sources = Vec::new();
    for index in 0..3 {
        let mut names = Vec::new();
        for slot in ["albedo", "normal", "orm", "emissive", "transparent"] {
            let name = format!("{slot}-{index}.png");
            std::fs::copy(
                f.root.join("assets/materials/color.png"),
                f.root.join("assets/materials").join(&name),
            )
            .unwrap();
            sources.push(format!("assets/materials/{name}"));
            names.push(if linked {
                name
            } else {
                sources.last().unwrap().clone()
            });
        }
        let config = MaterialConfig {
            albedo_texture: Some(names[0].clone()),
            normal_texture: Some(names[1].clone()),
            orm_texture: Some(names[2].clone()),
            emissive_texture: Some(names[3].clone()),
            transparent: Some(TransparentMaterialConfig {
                texture: Some(names[4].clone()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let assignment = if linked {
            let path = format!("assets/materials/material-{index}.ron");
            std::fs::write(f.root.join(&path), ron::ser::to_string(&config).unwrap()).unwrap();
            MaterialAssignment::Asset { path }
        } else {
            MaterialAssignment::Inline {
                material: config.clone(),
            }
        };
        let prepared = resolve_material_assignment(&assignment, &f.root, &mut f.assets).unwrap();
        assert_eq!(prepared.material, config);
        assert_eq!(prepared.material_asset.is_some(), linked);
        let resolution = prepared.extracted_material.resolution.as_ref().unwrap();
        let bindings = [
            (
                &resolution.albedo,
                4,
                TextureUsage::Albedo,
                ColorSpace::Srgb,
            ),
            (
                &resolution.normal,
                1,
                TextureUsage::Normal,
                ColorSpace::Linear,
            ),
            (&resolution.orm, 2, TextureUsage::Orm, ColorSpace::Linear),
            (
                &resolution.emissive,
                3,
                TextureUsage::Emissive,
                ColorSpace::Srgb,
            ),
        ];
        for (binding, slot, usage, color_space) in bindings {
            let source = &sources[index * 5 + slot];
            assert_eq!(
                binding.key.path(),
                f.root
                    .join(source)
                    .canonicalize()
                    .unwrap()
                    .to_str()
                    .unwrap()
            );
            assert_eq!(binding.key.usage(), usage);
            assert_eq!(binding.color_space, color_space);
            assert_eq!(binding.handle, resolution.albedo.handle);
            assert_eq!(
                &f.assets.get(binding.handle.unwrap()).unwrap().data[..4],
                &[128, 64, 32, 255]
            );
        }
        assert_eq!(
            prepared.transparent_material.unwrap().texture,
            resolution.albedo.handle
        );
        assert_eq!(f.assets.texture_snapshot_count(), 1);
        assert_eq!(f.assets.texture_snapshot_bytes(), 16);
    }
    // A request for a never-loaded source queues work but has no ready CPU value.
    // Probe after all resolutions so the probe itself cannot affect preparation.
    for source in sources {
        let (handle, _) = f.assets.request::<CpuTexture>(&source).unwrap();
        assert!(
            f.assets.get(handle).is_none(),
            "editor resolution must not retain ordinary source pixels: {source}"
        );
    }
}

#[test]
fn inline_duplicate_content_paths_retain_only_budgeted_snapshots() {
    duplicate_paths_do_not_load_source_textures(false);
}

#[test]
fn linked_duplicate_content_paths_retain_only_budgeted_snapshots() {
    duplicate_paths_do_not_load_source_textures(true);
}
