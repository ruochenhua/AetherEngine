//! Material assets resolved against their texture dependencies.

#[path = "material_asset/gpu.rs"]
mod gpu;

#[cfg(test)]
#[path = "material_asset/tests.rs"]
mod tests;

use crate::asset::{Asset, AssetError, AssetId, AssetKind, AssetManager};
use crate::renderer::transparent::TransparentMaterial;
use crate::scene::config::MaterialConfig;
use crate::scene::material::{
    MaterialResolution, MaterialResolveError, MaterialResolver, TextureFallback, TextureUsage,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use gpu::{GpuMaterial, GpuMaterialCache};

/// CPU representation of a material document and its resolved T3 bindings.
#[derive(Clone, Debug)]
pub struct MaterialAsset {
    /// Source material parameters.
    config: MaterialConfig,
    resolution: Option<MaterialResolution>,
    dependencies: Vec<MaterialDependency>,
}

/// A texture dependency and the resolution status recorded for this material.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterialDependency {
    /// Stable texture identity.
    pub asset: AssetId,
    /// Material slot that consumes this texture.
    pub usage: TextureUsage,
    /// Whether the dependency loaded or selected the T3 fallback.
    pub status: MaterialDependencyStatus,
}

/// Resolution state for one material texture dependency.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaterialDependencyStatus {
    /// The texture exists and decoded successfully.
    Resolved,
    /// The path is absent and the T3 usage fallback is selected.
    MissingFallback(TextureFallback),
}

impl MaterialAsset {
    /// Create a material asset from an already decoded material config.
    pub fn from_config(config: MaterialConfig) -> Self {
        Self {
            config,
            resolution: None,
            dependencies: Vec::new(),
        }
    }

    /// Borrow the decoded source material parameters.
    pub fn config(&self) -> &MaterialConfig {
        &self.config
    }

    /// Return the resolved T3 material, if dependency resolution has completed.
    pub fn resolution(&self) -> Option<&MaterialResolution> {
        self.resolution.as_ref()
    }

    /// Return the typed dependencies produced by the last successful resolution.
    pub fn dependencies(&self) -> &[MaterialDependency] {
        &self.dependencies
    }

    /// Resolve textures relative to the material file and record missing fallbacks.
    pub fn resolve(
        &mut self,
        material_id: &AssetId,
        project_root: &Path,
        texture_assets: &mut AssetManager,
    ) -> Result<(), AssetError> {
        if material_id.kind() != AssetKind::Material {
            return Err(AssetError::StaleHandle);
        }
        let relative_material = Path::new(material_id.path().as_str());
        let relative_root = relative_material.parent().unwrap_or_else(|| Path::new("."));
        let material_root = project_root.join(relative_root);
        let resolver = MaterialResolver::new(&material_root);
        let mut resolved_config = self.config.clone();
        if let Some(texture) = resolved_config
            .transparent
            .as_ref()
            .and_then(|transparent| transparent.texture.as_ref())
        {
            resolved_config.albedo_texture = Some(texture.clone());
        }
        let mut dependencies = Vec::new();
        for (raw_path, usage) in configured_textures(&resolved_config) {
            if let Some(path) = raw_path {
                let relative_path = relative_root.join(path);
                let asset =
                    AssetId::from_path(AssetKind::CpuTexture, project_root, &relative_path)?;
                dependencies.push((asset, usage));
            }
        }

        let resolution = resolver
            .resolve(&resolved_config, texture_assets)
            .map_err(|error| resolve_error(error, project_root))?;
        if let Some(config) = &self.config.transparent {
            TransparentMaterial {
                base_color: self.config.albedo,
                texture: resolution.material.albedo,
                blend: config.blend,
                alpha_cutoff: config.alpha_cutoff,
            }
            .validate()
            .map_err(|error| {
                AssetError::Decode(format!("invalid transparent material: {error:?}"))
            })?;
        }
        let mut resolved_dependencies = Vec::with_capacity(dependencies.len());
        for (asset, usage) in dependencies {
            let binding = binding_for(&resolution, usage);
            let status = if binding.handle.is_some() {
                MaterialDependencyStatus::Resolved
            } else {
                MaterialDependencyStatus::MissingFallback(binding.fallback)
            };
            resolved_dependencies.push(MaterialDependency {
                asset,
                usage,
                status,
            });
        }
        self.resolution = Some(resolution);
        self.dependencies = resolved_dependencies;
        Ok(())
    }
}

impl Asset for MaterialAsset {
    const KIND: AssetKind = AssetKind::Material;

    fn load(path: &Path) -> anyhow::Result<Self> {
        let source = std::fs::read_to_string(path)?;
        let config = ron::from_str::<MaterialConfig>(&source)?;
        Ok(Self::from_config(config))
    }
}

/// Resolve a worker result before applying it to the material AssetStore.
///
/// A corrupt texture becomes an `AssetError::Dependency` attached to the
/// material request, so `AssetStore::apply_result` leaves its current generation
/// available as the last-known-good value.
pub fn resolve_material_result(
    result: crate::asset::AssetResult,
    project_root: &Path,
    texture_assets: &mut AssetManager,
) -> crate::asset::AssetResult {
    match result {
        crate::asset::AssetResult::Ready { request, payload }
            if request.asset.kind() == AssetKind::Material =>
        {
            let Some(material) = payload.as_any().downcast_ref::<MaterialAsset>() else {
                return crate::asset::AssetResult::Ready { request, payload };
            };
            let mut resolved = material.clone();
            match resolved.resolve(&request.asset, project_root, texture_assets) {
                Ok(()) => crate::asset::AssetResult::Ready {
                    request,
                    payload: Arc::new(resolved),
                },
                Err(error) => crate::asset::AssetResult::Failed { request, error },
            }
        }
        other => other,
    }
}

fn configured_textures(config: &MaterialConfig) -> [(Option<&str>, TextureUsage); 4] {
    [
        (config.albedo_texture.as_deref(), TextureUsage::Albedo),
        (config.normal_texture.as_deref(), TextureUsage::Normal),
        (config.orm_texture.as_deref(), TextureUsage::Orm),
        (config.emissive_texture.as_deref(), TextureUsage::Emissive),
    ]
}

fn binding_for(
    resolution: &MaterialResolution,
    usage: TextureUsage,
) -> &crate::scene::material::TextureBinding {
    match usage {
        TextureUsage::Albedo => &resolution.albedo,
        TextureUsage::Normal => &resolution.normal,
        TextureUsage::Orm => &resolution.orm,
        TextureUsage::Emissive => &resolution.emissive,
    }
}

fn resolve_error(error: MaterialResolveError, root: &Path) -> AssetError {
    match error {
        MaterialResolveError::TextureLoad { path, message, .. } => {
            let resolved = PathBuf::from(&path);
            let dependency = resolved.strip_prefix(root).ok().and_then(|relative| {
                AssetId::from_path(AssetKind::CpuTexture, root, relative).ok()
            });
            match dependency {
                Some(asset) => AssetError::Dependency {
                    asset,
                    cause: Box::new(AssetError::Decode(message)),
                },
                None => AssetError::Decode(format!(
                    "texture dependency path could not be canonicalized: {message}"
                )),
            }
        }
        error => AssetError::Decode(error.to_string()),
    }
}
