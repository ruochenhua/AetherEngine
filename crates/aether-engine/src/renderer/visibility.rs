//! Deterministic CPU visibility extraction and screen-height LOD selection.

use crate::ecs::{
    components::{MeshHandle, MeshSource, Transform, Visibility},
    World,
};
use crate::math::{Aabb, Frustum, Mat4};
use crate::renderer::transparent::{TransparentBlendMode, TransparentMaterial};
use crate::renderer::{camera::FlyCamera, renderable::MaterialUniform};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Pixel-height boundary between high and medium detail.
pub const LOD_HIGH_PIXELS: f32 = 256.0;
/// Pixel-height boundary between medium and low detail.
pub const LOD_MEDIUM_PIXELS: f32 = 64.0;
/// Relative hysteresis applied to both sides of each LOD boundary.
pub const LOD_HYSTERESIS: f32 = 0.10;

/// Detail bucket assigned to a visible entity.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u8)]
pub enum LodLevel {
    /// Mesh occupies at least 256 pixels high.
    High = 0,
    /// Mesh occupies between the two thresholds.
    #[default]
    Medium = 1,
    /// Mesh occupies less than 64 pixels high.
    Low = 2,
}

impl LodLevel {
    /// Stable compact identifier for reports and draw-item contracts.
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Stable description of the mesh/material identity used by a render item.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BatchKey {
    /// Built-in identifier or normalized asset path.
    pub mesh_source: String,
    /// SHA-256 of the material's exact GPU uniform bytes.
    pub material_sha256: [u8; 32],
    /// Opaque, alpha, or additive pipeline class.
    pub transparent_blend: Option<u8>,
    /// Optional alpha cutoff encoded as its exact f32 bits.
    pub alpha_cutoff_bits: Option<u32>,
}

/// One renderable candidate before frustum filtering.
#[derive(Clone, Debug, PartialEq)]
pub struct VisibilityCandidate {
    /// Full generational entity identity.
    pub entity_bits: u64,
    /// Stable mesh/material batch identity.
    pub batch_key: BatchKey,
    /// World-space bounds after entity transform.
    pub bounds: Aabb,
    /// ECS visibility switch.
    pub enabled: bool,
}

/// A visible entity ready for batch construction.
#[derive(Clone, Debug, PartialEq)]
pub struct VisibleItem {
    /// Full generational entity identity.
    pub entity_bits: u64,
    /// Stable mesh/material batch identity.
    pub batch_key: BatchKey,
    /// Selected screen-height LOD level.
    pub lod: LodLevel,
    /// World-space bounds after entity transform.
    pub bounds: Aabb,
}

/// Per-frame visibility result, ordered by entity identity for reproducibility.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VisibilityFrame {
    /// Visible render items. Order is stable and independent of ECS query order.
    pub items: Vec<VisibleItem>,
    /// Number of disabled or out-of-frustum renderable entities.
    pub culled_entities: u32,
    /// Selected LOD for each visible entity.
    pub lod_by_entity: BTreeMap<u64, LodLevel>,
}

impl VisibilityFrame {
    /// Number of renderable entities retained for this camera.
    pub fn visible_entities(&self) -> u32 {
        self.items.len().min(u32::MAX as usize) as u32
    }

    /// Count visible entities in each LOD bucket as `[high, medium, low]`.
    pub fn lod_counts(&self) -> [u32; 3] {
        let mut counts = [0_u32; 3];
        for item in &self.items {
            counts[item.lod.as_u8() as usize] = counts[item.lod.as_u8() as usize].saturating_add(1);
        }
        counts
    }
}

/// Independent controls for culling and LOD selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VisibilityPolicy {
    /// Reject renderables fully outside the current camera frustum.
    pub frustum_culling: bool,
    /// Assign screen-height LOD buckets with hysteresis.
    pub lod_enabled: bool,
}

impl Default for VisibilityPolicy {
    fn default() -> Self {
        Self {
            frustum_culling: true,
            lod_enabled: true,
        }
    }
}

/// Stateful per-entity LOD history; stale entities are pruned on extraction.
#[derive(Debug, Default)]
pub struct LodHistory {
    levels: HashMap<u64, LodLevel>,
}

impl LodHistory {
    /// Drop state for entities that no longer exist in the world.
    pub fn retain_entities(&mut self, entity_bits: &HashSet<u64>) {
        self.levels.retain(|entity, _| entity_bits.contains(entity));
    }

    /// Forget all accumulated LOD state, for example when swapping scenes.
    pub fn clear(&mut self) {
        self.levels.clear();
    }

    /// Current state for one entity, if it has been seen before.
    pub fn get(&self, entity_bits: u64) -> Option<LodLevel> {
        self.levels.get(&entity_bits).copied()
    }

    fn select(&mut self, entity_bits: u64, screen_height: f32, enabled: bool) -> LodLevel {
        if !enabled {
            return LodLevel::High;
        }
        let selected = select_lod(self.levels.get(&entity_bits).copied(), screen_height);
        self.levels.insert(entity_bits, selected);
        selected
    }
}

/// Apply frustum and LOD policy to candidates using precomputed camera matrices.
pub fn evaluate_visibility(
    candidates: impl IntoIterator<Item = VisibilityCandidate>,
    view_projection: Mat4,
    frustum: &Frustum,
    viewport_height: u32,
    policy: VisibilityPolicy,
    history: &mut LodHistory,
) -> VisibilityFrame {
    let candidates: Vec<_> = candidates.into_iter().collect();
    let present_entities: HashSet<_> = candidates.iter().map(|item| item.entity_bits).collect();
    history.retain_entities(&present_entities);
    if !policy.lod_enabled {
        history.clear();
    }

    let mut frame = VisibilityFrame::default();
    for candidate in candidates {
        if !candidate.enabled
            || (policy.frustum_culling
                && candidate.bounds.intersects_frustum(frustum)
                    == crate::math::CullingVisibility::Invisible)
        {
            frame.culled_entities = frame.culled_entities.saturating_add(1);
            continue;
        }

        let pixel_height =
            projected_height_pixels(&candidate.bounds, view_projection, viewport_height);
        let lod = history.select(candidate.entity_bits, pixel_height, policy.lod_enabled);
        frame.lod_by_entity.insert(candidate.entity_bits, lod);
        frame.items.push(VisibleItem {
            entity_bits: candidate.entity_bits,
            batch_key: candidate.batch_key,
            lod,
            bounds: candidate.bounds,
        });
    }
    frame.items.sort_by_key(|item| item.entity_bits);
    frame
}

/// Extract visibility candidates from renderable ECS entities and apply policy.
pub fn extract_visibility(
    world: &World,
    camera: &FlyCamera,
    viewport_width: u32,
    viewport_height: u32,
    policy: VisibilityPolicy,
    history: &mut LodHistory,
) -> VisibilityFrame {
    let aspect = if viewport_height == 0 {
        1.0
    } else {
        viewport_width.max(1) as f32 / viewport_height as f32
    };
    let view_projection = camera.projection_matrix(aspect) * camera.view_matrix();
    let frustum = Frustum::from_view_projection(view_projection);
    let candidates = world
        .query::<(
            hecs::Entity,
            &Transform,
            &MeshHandle,
            &MaterialUniform,
            &Visibility,
            Option<&TransparentMaterial>,
        )>()
        .iter()
        .map(
            |(entity, transform, mesh, material, visibility, transparent)| {
                let model = Mat4::from_scale_rotation_translation(
                    transform.scale,
                    transform.rotation,
                    transform.translation,
                );
                VisibilityCandidate {
                    entity_bits: entity.to_bits().get(),
                    batch_key: make_batch_key(mesh, material, transparent),
                    bounds: mesh.mesh.aabb.transform(model),
                    enabled: visibility.0,
                }
            },
        )
        .collect::<Vec<_>>();
    evaluate_visibility(
        candidates,
        view_projection,
        &frustum,
        viewport_height,
        policy,
        history,
    )
}

/// Project a world-space AABB to screen height in physical pixels.
pub fn projected_height_pixels(bounds: &Aabb, view_projection: Mat4, viewport_height: u32) -> f32 {
    if viewport_height == 0 {
        return 0.0;
    }
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut crosses_camera_plane = false;
    for corner in bounds.corners() {
        let clip = view_projection * corner.extend(1.0);
        if !clip.w.is_finite() || clip.w <= f32::EPSILON {
            crosses_camera_plane = true;
            continue;
        }
        let ndc_y = clip.y / clip.w;
        if ndc_y.is_finite() {
            min_y = min_y.min(ndc_y);
            max_y = max_y.max(ndc_y);
        }
    }
    if crosses_camera_plane {
        return viewport_height as f32;
    }
    if !min_y.is_finite() || !max_y.is_finite() {
        return 0.0;
    }
    (((max_y - min_y).abs() * 0.5) * viewport_height as f32).clamp(0.0, viewport_height as f32)
}

/// Choose or hysteretically update a detail level from projected pixel height.
pub fn select_lod(current: Option<LodLevel>, screen_height: f32) -> LodLevel {
    if !screen_height.is_finite() || screen_height < 0.0 {
        return LodLevel::Low;
    }
    let Some(mut level) = current else {
        return if screen_height >= LOD_HIGH_PIXELS {
            LodLevel::High
        } else if screen_height >= LOD_MEDIUM_PIXELS {
            LodLevel::Medium
        } else {
            LodLevel::Low
        };
    };

    loop {
        let next = match level {
            LodLevel::High if screen_height <= LOD_HIGH_PIXELS * (1.0 - LOD_HYSTERESIS) => {
                LodLevel::Medium
            }
            LodLevel::Medium if screen_height >= LOD_HIGH_PIXELS * (1.0 + LOD_HYSTERESIS) => {
                LodLevel::High
            }
            LodLevel::Medium if screen_height <= LOD_MEDIUM_PIXELS * (1.0 - LOD_HYSTERESIS) => {
                LodLevel::Low
            }
            LodLevel::Low if screen_height >= LOD_MEDIUM_PIXELS * (1.0 + LOD_HYSTERESIS) => {
                LodLevel::Medium
            }
            _ => level,
        };
        if next == level {
            return level;
        }
        level = next;
    }
}

fn make_batch_key(
    mesh: &MeshHandle,
    material: &MaterialUniform,
    transparent: Option<&TransparentMaterial>,
) -> BatchKey {
    let mesh_source = match &mesh.source {
        MeshSource::Builtin(name) => format!("builtin:{name}"),
        MeshSource::File(path) => format!("file:{}", path.replace('\\', "/")),
    };
    let (transparent_blend, alpha_cutoff_bits) = transparent.map_or((None, None), |value| {
        let blend = match value.blend {
            TransparentBlendMode::Alpha => 1,
            TransparentBlendMode::Additive => 2,
        };
        (Some(blend), value.alpha_cutoff.map(f32::to_bits))
    });
    BatchKey {
        mesh_source,
        material_sha256: Sha256::digest(bytemuck::bytes_of(material)).into(),
        transparent_blend,
        alpha_cutoff_bits,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{look_at, perspective_projection, Vec3};

    fn candidate(entity_bits: u64, bounds: Aabb) -> VisibilityCandidate {
        VisibilityCandidate {
            entity_bits,
            batch_key: BatchKey::default(),
            bounds,
            enabled: true,
        }
    }

    #[test]
    fn lod_boundaries_select_three_detail_buckets() {
        assert_eq!(select_lod(None, 256.0), LodLevel::High);
        assert_eq!(select_lod(None, 255.99), LodLevel::Medium);
        assert_eq!(select_lod(None, 64.0), LodLevel::Medium);
        assert_eq!(select_lod(None, 63.99), LodLevel::Low);
    }

    #[test]
    fn lod_hysteresis_holds_within_ten_percent_bands() {
        assert_eq!(select_lod(Some(LodLevel::High), 230.5), LodLevel::High);
        assert_eq!(select_lod(Some(LodLevel::High), 230.0), LodLevel::Medium);
        assert_eq!(select_lod(Some(LodLevel::Medium), 281.5), LodLevel::Medium);
        assert_eq!(select_lod(Some(LodLevel::Medium), 281.7), LodLevel::High);
        assert_eq!(select_lod(Some(LodLevel::Medium), 57.7), LodLevel::Medium);
        assert_eq!(select_lod(Some(LodLevel::Medium), 57.5), LodLevel::Low);
        assert_eq!(select_lod(Some(LodLevel::Low), 70.3), LodLevel::Low);
        assert_eq!(select_lod(Some(LodLevel::Low), 70.5), LodLevel::Medium);
    }

    #[test]
    fn lod_handles_large_camera_jumps_without_skipping_levels() {
        assert_eq!(select_lod(Some(LodLevel::High), 10.0), LodLevel::Low);
        assert_eq!(select_lod(Some(LodLevel::Low), 500.0), LodLevel::High);
    }

    #[test]
    fn aabb_tangent_to_frustum_boundary_remains_visible() {
        let frustum = Frustum::from_view_projection(Mat4::IDENTITY);
        let bounds = Aabb::from_min_max(Vec3::new(1.0, -0.5, -0.5), Vec3::new(1.2, 0.5, 0.5));
        let mut history = LodHistory::default();
        let frame = evaluate_visibility(
            [candidate(4, bounds)],
            Mat4::IDENTITY,
            &frustum,
            720,
            VisibilityPolicy::default(),
            &mut history,
        );
        assert_eq!(frame.visible_entities(), 1);
        assert_eq!(frame.culled_entities, 0);
    }

    #[test]
    fn camera_move_changes_visible_and_culled_entity_counts() {
        let bounds = Aabb::from_min_max(Vec3::new(-0.5, -0.5, -10.5), Vec3::new(0.5, 0.5, -9.5));
        let candidate = candidate(9, bounds);
        let view = look_at(Vec3::ZERO, Vec3::NEG_Z, Vec3::Y);
        let projection = perspective_projection(90.0_f32.to_radians(), 1.0, 0.1, 100.0);
        let centered_vp = projection * view;
        let moved_vp = projection
            * look_at(
                Vec3::new(20.0, 0.0, 0.0),
                Vec3::new(20.0, 0.0, -1.0),
                Vec3::Y,
            );
        let centered_frustum = Frustum::from_view_projection(centered_vp);
        let moved_frustum = Frustum::from_view_projection(moved_vp);
        let mut history = LodHistory::default();
        let centered = evaluate_visibility(
            [candidate.clone()],
            centered_vp,
            &centered_frustum,
            720,
            VisibilityPolicy::default(),
            &mut history,
        );
        let moved = evaluate_visibility(
            [candidate],
            moved_vp,
            &moved_frustum,
            720,
            VisibilityPolicy::default(),
            &mut history,
        );
        assert_eq!(centered.visible_entities(), 1);
        assert_eq!(moved.visible_entities(), 0);
        assert_eq!(moved.culled_entities, 1);
    }

    #[test]
    fn generated_100_1k_and_10k_candidate_frames_are_complete_and_ordered() {
        let frustum = Frustum::from_view_projection(Mat4::IDENTITY);
        for count in [100_usize, 1_000, 10_000] {
            let bounds = Aabb::from_min_max(Vec3::splat(-0.25), Vec3::splat(0.25));
            let candidates = (0..count).rev().map(|id| candidate(id as u64, bounds));
            let mut history = LodHistory::default();
            let frame = evaluate_visibility(
                candidates,
                Mat4::IDENTITY,
                &frustum,
                720,
                VisibilityPolicy::default(),
                &mut history,
            );
            assert_eq!(frame.visible_entities(), count as u32);
            assert_eq!(frame.culled_entities, 0);
            assert!(frame
                .items
                .windows(2)
                .all(|pair| pair[0].entity_bits < pair[1].entity_bits));
        }
    }

    #[test]
    fn visibility_switch_preserves_legacy_unfiltered_path() {
        let frustum = Frustum::from_view_projection(Mat4::IDENTITY);
        let outside = Aabb::from_min_max(Vec3::new(3.0, -0.5, -0.5), Vec3::new(4.0, 0.5, 0.5));
        let mut history = LodHistory::default();
        let frame = evaluate_visibility(
            [candidate(1, outside)],
            Mat4::IDENTITY,
            &frustum,
            720,
            VisibilityPolicy {
                frustum_culling: false,
                lod_enabled: false,
            },
            &mut history,
        );
        assert_eq!(frame.visible_entities(), 1);
        assert_eq!(frame.items[0].lod, LodLevel::High);
        assert_eq!(frame.culled_entities, 0);
    }
}
