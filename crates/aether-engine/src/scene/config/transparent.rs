//! Serializable configuration for the general transparent material path.

use serde::{Deserialize, Serialize};

pub use crate::renderer::transparent::TransparentBlendMode;

/// Scene-file configuration for a general transparent material.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransparentMaterialConfig {
    /// Fixed Alpha or Additive blend mode.
    #[serde(default)]
    pub blend: TransparentBlendMode,
    /// Optional alpha discard cutoff.
    #[serde(default)]
    pub alpha_cutoff: Option<f32>,
    /// Optional albedo texture path.
    #[serde(default)]
    pub texture: Option<String>,
}

impl Default for TransparentMaterialConfig {
    fn default() -> Self {
        Self {
            blend: TransparentBlendMode::Alpha,
            alpha_cutoff: None,
            texture: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_material_config_roundtrips_alpha_and_additive_modes() {
        for blend in [TransparentBlendMode::Alpha, TransparentBlendMode::Additive] {
            let config = TransparentMaterialConfig {
                blend,
                alpha_cutoff: Some(0.5),
                texture: None,
            };
            let encoded = ron::ser::to_string(&config).unwrap();
            let decoded: TransparentMaterialConfig = ron::de::from_str(&encoded).unwrap();
            assert_eq!(decoded, config);
        }
    }
}
