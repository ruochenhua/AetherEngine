use std::{
    fs,
    path::{Component, Path, PathBuf},
};

/// Resolve an OBJ that can be loaded without following symlinks outside assets/.
/// Its MTL files and diffuse textures must also be project-local files.
pub(super) fn validate_obj_asset(
    relative_path: &Path,
    project_root: &Path,
) -> anyhow::Result<PathBuf> {
    if relative_path.is_absolute()
        || relative_path.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
        || relative_path.components().next() != Some(Component::Normal("assets".as_ref()))
        || !relative_path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("obj"))
    {
        anyhow::bail!("only project-relative OBJ files under assets/ can be placed");
    }

    let root_metadata = fs::symlink_metadata(project_root)?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        anyhow::bail!("project root is missing or is a symlink");
    }
    let canonical_root = project_root.canonicalize()?;
    let assets_root = canonical_root.join("assets");
    let assets_metadata = fs::symlink_metadata(&assets_root)?;
    if assets_metadata.file_type().is_symlink() || !assets_metadata.is_dir() {
        anyhow::bail!("project assets directory is missing or is a symlink");
    }
    let obj = resolve_local_file(&canonical_root, relative_path, &assets_root)?;
    let reader = std::io::BufReader::new(fs::File::open(&obj)?);
    for line in std::io::BufRead::lines(reader) {
        let line = line?;
        let mut fields = line.split_whitespace();
        if let Some("mtllib") = fields.next() {
            for library in fields {
                let library_path =
                    resolve_local_file(obj.parent().unwrap(), Path::new(library), &assets_root)?;
                validate_mtl_textures(&library_path, &assets_root)?;
            }
        }
    }
    Ok(obj)
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

fn validate_mtl_textures(mtl_path: &Path, assets_root: &Path) -> anyhow::Result<()> {
    let reader = std::io::BufReader::new(fs::File::open(mtl_path)?);
    for line in std::io::BufRead::lines(reader) {
        let line = line?;
        let mut fields = line.split_whitespace();
        if fields.next() == Some("map_Kd") {
            let texture = fields
                .last()
                .ok_or_else(|| anyhow::anyhow!("MTL diffuse texture path is missing"))?;
            if texture.starts_with('-') {
                anyhow::bail!("MTL diffuse texture options are not supported for dropped models");
            }
            resolve_local_file(mtl_path.parent().unwrap(), Path::new(texture), assets_root)?;
        }
    }
    Ok(())
}

fn resolve_local_file(base: &Path, relative: &Path, assets_root: &Path) -> anyhow::Result<PathBuf> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
        anyhow::bail!("OBJ material dependencies must stay inside assets/");
    }
    let mut cursor = base.to_path_buf();
    for component in relative.components() {
        if let Component::Normal(name) = component {
            cursor.push(name);
            let metadata = fs::symlink_metadata(&cursor)?;
            if metadata.file_type().is_symlink() {
                anyhow::bail!("symbolic links are not supported for dropped model dependencies");
            }
        }
    }
    let canonical = cursor.canonicalize()?;
    if !canonical.starts_with(assets_root) || !canonical.is_file() {
        anyhow::bail!("OBJ material dependency is outside assets/ or is not a file");
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

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
