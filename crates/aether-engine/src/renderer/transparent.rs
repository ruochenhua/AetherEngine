//! CPU-side contracts for the single-layer transparent renderer.

use crate::asset::texture::CpuTexture;
use crate::asset::Handle;
use serde::{Deserialize, Serialize};

/// Supported first-generation transparent blend modes.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub enum TransparentBlendMode {
    /// Premultiplied source-over compositing.
    #[default]
    Alpha,
    /// Additive emission-like compositing.
    Additive,
}

/// Blend factors used by the fixed transparent target contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendFactor {
    /// Factor one.
    One,
    /// Factor zero.
    Zero,
    /// One minus source alpha.
    OneMinusSrcAlpha,
}

impl TransparentBlendMode {
    /// Return the fixed RGB source/destination factors.
    pub const fn rgb_factors(self) -> (BlendFactor, BlendFactor) {
        match self {
            Self::Alpha => (BlendFactor::One, BlendFactor::OneMinusSrcAlpha),
            Self::Additive => (BlendFactor::One, BlendFactor::One),
        }
    }

    /// Return the fixed alpha source/destination factors.
    pub const fn alpha_factors(self) -> (BlendFactor, BlendFactor) {
        match self {
            Self::Alpha => (BlendFactor::One, BlendFactor::OneMinusSrcAlpha),
            Self::Additive => (BlendFactor::Zero, BlendFactor::One),
        }
    }
}

/// Runtime material data consumed by the transparent pass.
#[derive(Clone, Debug, PartialEq)]
pub struct TransparentMaterial {
    /// Straight-alpha linear base color.
    pub base_color: [f32; 4],
    /// Optional albedo texture handle.
    pub texture: Option<Handle<CpuTexture>>,
    /// Fixed blend mode.
    pub blend: TransparentBlendMode,
    /// Optional alpha discard cutoff.
    pub alpha_cutoff: Option<f32>,
}

impl Default for TransparentMaterial {
    fn default() -> Self {
        Self {
            base_color: [1.0; 4],
            texture: None,
            blend: TransparentBlendMode::Alpha,
            alpha_cutoff: None,
        }
    }
}

impl TransparentMaterial {
    /// Convenience constructor used by pure contract tests.
    pub fn with_cutoff(alpha_cutoff: Option<f32>) -> Self {
        Self {
            alpha_cutoff,
            ..Self::default()
        }
    }

    /// Validate the fixed input contract before it reaches the GPU pass.
    pub fn validate(&self) -> Result<(), TransparentMaterialError> {
        if self.base_color.iter().any(|value| !value.is_finite()) {
            return Err(TransparentMaterialError::NonFiniteColor);
        }
        if let Some(cutoff) = self.alpha_cutoff {
            if !cutoff.is_finite() || !(0.0..=1.0).contains(&cutoff) {
                return Err(TransparentMaterialError::InvalidAlphaCutoff);
            }
        }
        Ok(())
    }
}

/// Invalid transparent material input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransparentMaterialError {
    /// A base color contains NaN or infinity.
    NonFiniteColor,
    /// Cutoff is not finite or outside the inclusive [0, 1] range.
    InvalidAlphaCutoff,
}

/// One transparent entity after camera-depth extraction.
#[derive(Clone, Debug, PartialEq)]
pub struct TransparentRenderItem {
    /// Stable ECS entity bits used as the deterministic tie-breaker.
    pub entity_bits: u64,
    /// Positive camera-space depth; larger values render first.
    pub view_depth: f32,
    /// Resolved transparent material.
    pub material: TransparentMaterial,
}

impl TransparentRenderItem {
    /// Construct one item for sorting and validation.
    pub fn new(entity_bits: u64, view_depth: f32, material: TransparentMaterial) -> Self {
        Self {
            entity_bits,
            view_depth,
            material,
        }
    }
}

/// Sort transparent items from far to near and report rejected depths.
pub fn sort_items(mut items: Vec<TransparentRenderItem>) -> (Vec<TransparentRenderItem>, Vec<u64>) {
    let mut rejected = Vec::new();
    items.retain(|item| {
        if item.view_depth.is_finite() {
            true
        } else {
            rejected.push(item.entity_bits);
            false
        }
    });
    items.sort_by(|left, right| {
        right
            .view_depth
            .total_cmp(&left.view_depth)
            .then_with(|| left.entity_bits.cmp(&right.entity_bits))
    });
    (items, rejected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_items_sort_far_to_near_with_entity_tie_break() {
        let items = vec![
            TransparentRenderItem::new(9, 2.0, TransparentMaterial::default()),
            TransparentRenderItem::new(4, 5.0, TransparentMaterial::default()),
            TransparentRenderItem::new(2, 5.0, TransparentMaterial::default()),
        ];

        let (sorted, rejected) = sort_items(items);

        assert!(rejected.is_empty());
        assert_eq!(
            sorted
                .iter()
                .map(|item| item.entity_bits)
                .collect::<Vec<_>>(),
            vec![2, 4, 9]
        );
    }

    #[test]
    fn non_finite_depth_is_rejected_without_entering_the_draw_list() {
        let items = vec![
            TransparentRenderItem::new(7, f32::NAN, TransparentMaterial::default()),
            TransparentRenderItem::new(8, 1.0, TransparentMaterial::default()),
        ];

        let (sorted, rejected) = sort_items(items);

        assert_eq!(sorted.len(), 1);
        assert_eq!(rejected, vec![7]);
    }

    #[test]
    fn alpha_cutoff_accepts_only_finite_values_inclusive_of_zero_and_one() {
        for cutoff in [Some(0.0), Some(1.0), None] {
            assert!(TransparentMaterial::with_cutoff(cutoff).validate().is_ok());
        }
        assert!(matches!(
            TransparentMaterial::with_cutoff(Some(f32::NAN)).validate(),
            Err(TransparentMaterialError::InvalidAlphaCutoff)
        ));
        assert!(matches!(
            TransparentMaterial::with_cutoff(Some(1.1)).validate(),
            Err(TransparentMaterialError::InvalidAlphaCutoff)
        ));
    }

    #[test]
    fn blend_contract_is_premultiplied_alpha_or_additive() {
        assert_eq!(
            TransparentBlendMode::Alpha.rgb_factors(),
            (BlendFactor::One, BlendFactor::OneMinusSrcAlpha)
        );
        assert_eq!(
            TransparentBlendMode::Additive.rgb_factors(),
            (BlendFactor::One, BlendFactor::One)
        );
    }
}
