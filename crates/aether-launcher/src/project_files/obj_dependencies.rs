use super::FileOperationError;
use std::path::{Component, Path, PathBuf};

pub(super) fn validate_obj_asset(
    project_root: &Path,
    relative_path: &Path,
) -> Result<PathBuf, FileOperationError> {
    let components = relative_path.components().collect::<Vec<_>>();
    if relative_path.is_absolute()
        || components
            .iter()
            .any(|part| !matches!(part, Component::Normal(_)))
        || components.first() != Some(&Component::Normal("assets".as_ref()))
        || !relative_path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("obj"))
    {
        return Err(FileOperationError::Path(
            "Only project-relative OBJ files under assets/ can be imported".into(),
        ));
    }

    let root_metadata = std::fs::symlink_metadata(project_root)?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(FileOperationError::Path(
            "Project root is missing or is a symlink".into(),
        ));
    }
    let canonical_root = project_root.canonicalize()?;
    let assets_root = canonical_root.join("assets");
    let assets_metadata = std::fs::symlink_metadata(&assets_root)?;
    if assets_metadata.file_type().is_symlink() || !assets_metadata.is_dir() {
        return Err(FileOperationError::Path(
            "Project assets directory is missing or is a symlink".into(),
        ));
    }
    let obj = resolve_local_file(&canonical_root, relative_path, &assets_root)?;
    let reader = std::io::BufReader::new(std::fs::File::open(&obj)?);
    for line in std::io::BufRead::lines(reader) {
        let line = line?;
        let mut fields = line
            .split('#')
            .next()
            .unwrap_or_default()
            .split_whitespace();
        if fields.next() == Some("mtllib") {
            for library in fields {
                let library_path =
                    resolve_local_file(obj.parent().unwrap(), Path::new(library), &assets_root)?;
                validate_mtl_textures(&library_path, &assets_root)?;
            }
        }
    }
    Ok(obj)
}

fn validate_mtl_textures(mtl_path: &Path, assets_root: &Path) -> Result<(), FileOperationError> {
    let reader = std::io::BufReader::new(std::fs::File::open(mtl_path)?);
    for line in std::io::BufRead::lines(reader) {
        let line = line?;
        let mut fields = line
            .split('#')
            .next()
            .unwrap_or_default()
            .split_whitespace();
        if fields.next() == Some("map_Kd") {
            let texture = fields.next_back().ok_or_else(|| {
                FileOperationError::ExternalDependency("MTL diffuse texture path is missing".into())
            })?;
            if texture.starts_with('-') {
                return Err(FileOperationError::Validation(
                    "MTL texture options are unsupported for imported OBJ packages".into(),
                ));
            }
            resolve_local_file(mtl_path.parent().unwrap(), Path::new(texture), assets_root)?;
        }
    }
    Ok(())
}

fn resolve_local_file(
    base: &Path,
    relative: &Path,
    assets_root: &Path,
) -> Result<PathBuf, FileOperationError> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
        return Err(FileOperationError::ExternalDependency(
            relative.display().to_string(),
        ));
    }
    let mut cursor = base.to_path_buf();
    for component in relative.components() {
        if let Component::Normal(name) = component {
            cursor.push(name);
            let metadata = std::fs::symlink_metadata(&cursor).map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    FileOperationError::ExternalDependency(relative.display().to_string())
                } else {
                    error.into()
                }
            })?;
            if metadata.file_type().is_symlink() {
                return Err(FileOperationError::Path(format!(
                    "Symbolic link dependency: {}",
                    cursor.display()
                )));
            }
        }
    }
    let canonical = cursor.canonicalize().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            FileOperationError::ExternalDependency(relative.display().to_string())
        } else {
            error.into()
        }
    })?;
    if !canonical.starts_with(assets_root) || !canonical.is_file() {
        return Err(FileOperationError::ExternalDependency(
            relative.display().to_string(),
        ));
    }
    Ok(canonical)
}
