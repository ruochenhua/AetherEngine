use std::path::{Path, PathBuf};

/// Resolve an OBJ that can be loaded without following symlinks outside assets/.
/// Its MTL files and diffuse textures must also be project-local files.
pub(super) fn validate_obj_asset(
    relative_path: &Path,
    project_root: &Path,
) -> anyhow::Result<PathBuf> {
    Ok(crate::project_files::validate_obj_asset(
        project_root,
        relative_path,
    )?)
}

pub(super) fn project_relative_texture_path(
    raw_path: &str,
    project_root: &Path,
) -> anyhow::Result<String> {
    let root = project_root.canonicalize()?;
    let raw = Path::new(raw_path);
    let absolute = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        root.join(raw)
    };
    let canonical = absolute.canonicalize()?;
    if !canonical.starts_with(root.join("assets")) || !canonical.is_file() {
        anyhow::bail!("OBJ diffuse texture is outside assets/ or is not a file");
    }
    Ok(canonical
        .strip_prefix(&root)?
        .to_string_lossy()
        .replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn project_cyborg_obj_and_its_local_texture_dependencies_are_supported() {
        use aether_engine::{
            asset::{mesh::CpuMesh, AssetManager},
            scene::{material::MaterialResolver, MaterialConfig},
        };

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let obj = validate_obj_asset(Path::new("assets/models/cyborg/cyborg.obj"), &root)
            .expect("the project Cyborg OBJ and its local MTL textures should be accepted");
        assert!(obj.ends_with("cyborg.obj"));

        let mut assets = AssetManager::with_project_root(&root);
        let mesh_handle = assets
            .load::<CpuMesh>("assets/models/cyborg/cyborg.obj")
            .expect("the OBJ's single material should load");
        let mesh = assets.get(mesh_handle).unwrap();
        assert_eq!(mesh.submeshes.len(), 1);
        let submesh = &mesh.submeshes[0];
        let material = MaterialConfig {
            albedo: submesh.material.base_color,
            roughness: submesh.material.roughness,
            metallic: submesh.material.metallic,
            albedo_texture: submesh.material.albedo_texture.clone(),
            ..MaterialConfig::default()
        };
        MaterialResolver::new(&root)
            .resolve(&material, &mut assets)
            .expect("the referenced local diffuse texture should resolve");
    }

    #[test]
    fn material_dependency_cannot_escape_assets() {
        let root = std::env::temp_dir().join(format!("aether_bad_mtl_{}", std::process::id()));
        let models = root.join("assets/models");
        fs::create_dir_all(&models).unwrap();
        fs::write(models.join("sample.obj"), "mtllib ../outside.mtl\n").unwrap();
        fs::write(root.join("assets/outside.mtl"), "newmtl Sample\n").unwrap();

        assert!(validate_obj_asset(Path::new("assets/models/sample.obj"), &root).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
