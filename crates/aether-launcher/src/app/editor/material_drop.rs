//! Host-side resolution: called after egui has captured a pending request.
use super::resource_path::validate_resource;
use crate::{
    app::{
        workspace::{MaterialDropRequest, MaterialDropTarget},
        App, LauncherState,
    },
    project_index::ResourceKind,
};
use aether_engine::{
    asset::{texture::CpuTexture, AssetManager, Handle},
    ecs::{
        components::{MaterialAssetRef, MaterialAssetStatus, MeshHandle, Name},
        World,
    },
    editor::{EditorContext, EditorOperation, MaterialAssignment, PreparedMaterial},
    renderer::{
        extracted_material::ExtractedMaterial, renderable::MaterialUniform,
        transparent::TransparentMaterial,
    },
    scene::{
        material::{MaterialResolution, MaterialResolver, TextureCacheKey, TextureUsage},
        MaterialConfig,
    },
};
use std::path::{Path, PathBuf};

pub(super) fn prepare_material_drop(
    request: &MaterialDropRequest,
    root: &Path,
    assets: &mut AssetManager,
    world: &World,
) -> anyhow::Result<(EditorOperation, PreparedMaterial)> {
    anyhow::ensure!(
        world.query_one::<&MeshHandle>(request.entity).get().is_ok(),
        "material drop target is not a Mesh"
    );
    let name = world.query_one::<&Name>(request.entity).get()?.0.clone();
    anyhow::ensure!(
        world
            .query::<&Name>()
            .iter()
            .filter(|n| n.0 == name)
            .count()
            == 1,
        "material target name must be unique"
    );
    let assignment = match request.target {
        MaterialDropTarget::Asset => {
            anyhow::ensure!(
                request.resource.kind == ResourceKind::Material,
                "Material Asset accepts Material resources only"
            );
            MaterialAssignment::Asset {
                path: request.resource.relative_path.clone(),
            }
        }
        MaterialDropTarget::Texture(slot) => {
            anyhow::ensure!(
                request.resource.kind == ResourceKind::Image,
                "texture slots accept Image resources only"
            );
            anyhow::ensure!(
                world
                    .query_one::<&MaterialAssetRef>(request.entity)
                    .get()
                    .is_err(),
                "linked material textures are read-only; use Detach and edit first"
            );
            validate_resource(
                Path::new(&request.resource.relative_path),
                root,
                ResourceKind::Image,
            )?;
            let mut material = world
                .query_one::<&MaterialConfig>(request.entity)
                .get()
                .ok()
                .cloned()
                .unwrap_or_default();
            let path = request.resource.relative_path.clone();
            match slot {
                TextureUsage::Albedo => {
                    material.albedo_texture = Some(path.clone());
                    if let Some(transparent) = material.transparent.as_mut() {
                        transparent.texture = Some(path);
                    }
                }
                TextureUsage::Normal => material.normal_texture = Some(path),
                TextureUsage::Orm => material.orm_texture = Some(path),
                TextureUsage::Emissive => material.emissive_texture = Some(path),
            }
            MaterialAssignment::Inline { material }
        }
    };
    let prepared = resolve_material_assignment(&assignment, root, assets)?;
    Ok((
        EditorOperation::SetMaterial {
            entity_name: name,
            assignment,
        },
        prepared,
    ))
}

pub(super) fn resolve_material_assignment(
    assignment: &MaterialAssignment,
    root: &Path,
    assets: &mut AssetManager,
) -> anyhow::Result<PreparedMaterial> {
    let canonical_root = root.canonicalize()?;
    anyhow::ensure!(
        assets.project_root().canonicalize()? == canonical_root,
        "asset manager is rooted at a different project directory"
    );
    let (material, parent, reference) = match assignment {
        MaterialAssignment::Asset { path } => {
            let document = validate_resource(Path::new(path), root, ResourceKind::Material)?;
            // Read the current document directly; a previously cached material is not authoritative.
            let config = super::material_document::decode(&std::fs::read_to_string(document)?)?;
            let parent = Path::new(path)
                .parent()
                .unwrap_or_else(|| Path::new("assets"));
            (config, parent, Some(MaterialAssetRef(path.clone())))
        }
        MaterialAssignment::Inline { material } => (material.clone(), Path::new(""), None),
    };
    // Resolve only scalars/fallbacks: source-path texture loads would retain an
    // unbudgeted duplicate CPU payload for every path, even for identical pixels.
    let mut resolution = validate_material_scalars(&material, assets)?;
    let albedo = material
        .transparent
        .as_ref()
        .and_then(|transparent| transparent.texture.as_ref())
        .or(material.albedo_texture.as_ref());
    for (binding, path) in [
        (&mut resolution.albedo, albedo),
        (&mut resolution.normal, material.normal_texture.as_ref()),
        (&mut resolution.orm, material.orm_texture.as_ref()),
        (&mut resolution.emissive, material.emissive_texture.as_ref()),
    ] {
        if let Some(path) = path {
            let dependency =
                super::resource_path::material_dependency(parent, Path::new(path), root)?;
            let source = canonical_root
                .join(dependency)
                .to_string_lossy()
                .replace('\\', "/");
            binding.key = TextureCacheKey::new(source, binding.usage);
        }
    }
    let snapshots = prepare_texture_snapshots(&material, parent, root, assets)?;
    let resolution = bind_texture_snapshots(resolution, &snapshots, &canonical_root)?;
    prepared_material(material, resolution, reference)
}

fn prepare_texture_snapshots(
    config: &MaterialConfig,
    parent: &Path,
    root: &Path,
    assets: &mut AssetManager,
) -> anyhow::Result<std::collections::BTreeMap<PathBuf, Handle<CpuTexture>>> {
    let mut validated = std::collections::BTreeMap::new();
    // Validate and decode every dependency before registering runtime snapshots.
    // The existing source cache and previous immutable versions remain untouched.
    for path in [
        &config.albedo_texture,
        &config.normal_texture,
        &config.orm_texture,
        &config.emissive_texture,
        &config.transparent.as_ref().and_then(|t| t.texture.clone()),
    ]
    .into_iter()
    .flatten()
    {
        let dependency = super::resource_path::material_dependency(parent, Path::new(path), root)?;
        if validated.contains_key(&dependency) {
            continue;
        }
        let file = validate_resource(&dependency, root, ResourceKind::Image)?;
        let decoded = super::image_decode::validate_image(&file)?;
        validated.insert(dependency, decoded);
    }
    let (paths, pixels): (Vec<_>, Vec<_>) = validated.into_iter().unzip();
    let handles = assets.register_texture_snapshots(pixels)?;
    Ok(paths.into_iter().zip(handles).collect())
}

fn validate_material_scalars(
    config: &MaterialConfig,
    assets: &mut AssetManager,
) -> anyhow::Result<MaterialResolution> {
    for (field, values) in [
        ("albedo", config.albedo.as_slice()),
        ("emissive", config.emissive.as_slice()),
    ] {
        anyhow::ensure!(
            values.iter().all(|value| value.is_finite()),
            "material {field} must be finite"
        );
    }
    for (field, value) in [
        ("roughness", config.roughness),
        ("metallic", config.metallic),
    ] {
        anyhow::ensure!(
            value.is_finite() && (0.0..=1.0).contains(&value),
            "material {field} must be finite and in [0, 1]"
        );
    }
    // Validate resolver-backed ranges and transparent scalars without touching
    // any texture asset or registering runtime snapshots.
    let mut scalars = config.clone();
    scalars.albedo_texture = None;
    scalars.normal_texture = None;
    scalars.orm_texture = None;
    scalars.emissive_texture = None;
    if let Some(transparent) = &mut scalars.transparent {
        transparent.texture = None;
    }
    let resolution = MaterialResolver::new(assets.project_root()).resolve(&scalars, assets)?;
    prepared_material(scalars, resolution.clone(), None)?;
    Ok(resolution)
}

fn bind_texture_snapshots(
    mut resolution: MaterialResolution,
    snapshots: &std::collections::BTreeMap<PathBuf, Handle<CpuTexture>>,
    canonical_root: &Path,
) -> anyhow::Result<MaterialResolution> {
    for binding in [
        &mut resolution.albedo,
        &mut resolution.normal,
        &mut resolution.orm,
        &mut resolution.emissive,
    ] {
        let source = Path::new(binding.key.path());
        if source.file_name().is_some_and(|name| name == "<fallback>") {
            continue;
        }
        let relative = source.strip_prefix(canonical_root)?;
        binding.handle = Some(*snapshots.get(relative).ok_or_else(|| {
            anyhow::anyhow!(
                "texture was not validated for material assignment: {}",
                source.display()
            )
        })?);
    }
    resolution.material.albedo = resolution.albedo.handle;
    resolution.material.normal = resolution.normal.handle;
    resolution.material.orm = resolution.orm.handle;
    resolution.material.emissive_texture = resolution.emissive.handle;
    Ok(resolution)
}

fn prepared_material(
    material: MaterialConfig,
    resolution: MaterialResolution,
    reference: Option<MaterialAssetRef>,
) -> anyhow::Result<PreparedMaterial> {
    let transparent_material = material.transparent.as_ref().map(|t| TransparentMaterial {
        base_color: material.albedo,
        texture: resolution.material.albedo,
        blend: t.blend,
        alpha_cutoff: t.alpha_cutoff,
    });
    if let Some(transparent) = &transparent_material {
        transparent
            .validate()
            .map_err(|e| anyhow::anyhow!("invalid transparent material: {e:?}"))?;
    }
    Ok(PreparedMaterial {
        material,
        uniform: MaterialUniform::from_resolution(&resolution),
        extracted_material: ExtractedMaterial::from_resolution(&resolution),
        material_asset_status: reference.as_ref().map(|_| MaterialAssetStatus::default()),
        material_asset: reference,
        transparent_material,
    })
}

pub(super) fn apply_pending_drop(app: &mut App) {
    let Some(request) = app.pending_material_drop.take() else {
        return;
    };
    if let Some(reason) = &app.editor_write_degraded {
        app.editor_operation_message = Some(format!("编辑器写入已停止：{reason}"));
        return;
    }
    let result = commit_drop(app, &request);
    match result {
        Ok(()) => {
            app.undo_action_order.push(super::UndoActionKind::Operation);
            app.redo_action_order.clear();
            app.editor_operation_message =
                Some("Material assignment saved to scene and editor log".into());
            tracing::info!(resource = %request.resource.relative_path, "material drop saved");
        }
        Err(error) => {
            super::mark_degraded(app, &error);
            app.editor_operation_message = Some(format!("Material drop rejected: {error}"));
            tracing::warn!(%error, "material drop rejected");
        }
    }
}

fn commit_drop(
    app: &mut App,
    request: &MaterialDropRequest,
) -> Result<(), aether_engine::editor::EditorError> {
    use aether_engine::editor::EditorError;
    let scene = app.cli.scene.as_deref().map(PathBuf::from).ok_or_else(|| {
        EditorError::InvalidMaterialOperation("save the scene before assigning resources".into())
    })?;
    let root = app.workspace.project_root().to_path_buf();
    let App {
        state,
        editor_history,
        asset_manager,
        ..
    } = app;
    let LauncherState::Running { world, lighting } = state else {
        return Err(EditorError::InvalidMaterialOperation(
            "no active scene".into(),
        ));
    };
    let (operation, prepared) = prepare_material_drop(request, &root, asset_manager, world)
        .map_err(|e| EditorError::InvalidMaterialOperation(e.to_string()))?;
    let log = super::operation_log_path(&scene);
    let name = scene.file_stem().unwrap_or_default().to_string_lossy();
    editor_history.apply_prepared_material(
        &mut EditorContext::new(world, lighting, &name, &scene, &log),
        operation,
        prepared,
    )
}
