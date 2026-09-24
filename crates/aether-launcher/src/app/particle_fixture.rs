//! Static particle frame for the renderer-only billboard acceptance scene.

use aether_engine::asset::{texture::CpuTexture, Handle};
use aether_engine::particles::{FrameStatus, ParticleFrame, ParticleRenderItem};
use aether_engine::renderer::transparent::TransparentBlendMode;
use std::sync::Arc;

pub(super) fn billboard_frame(texture: Option<Handle<CpuTexture>>) -> Arc<ParticleFrame> {
    let particles = [
        (
            [-1.2, 0.5, 0.0],
            [1.0, 0.18, 0.08, 0.68],
            TransparentBlendMode::Alpha,
        ),
        (
            [-0.4, 0.5, 0.0],
            [1.0, 0.62, 0.08, 0.5],
            TransparentBlendMode::Additive,
        ),
        (
            [0.4, 0.5, 0.0],
            [0.12, 0.72, 1.0, 0.58],
            TransparentBlendMode::Alpha,
        ),
        (
            [1.2, 0.5, 0.0],
            [0.76, 0.24, 1.0, 0.5],
            TransparentBlendMode::Additive,
        ),
        (
            [-0.8, -0.55, -0.6],
            [0.24, 1.0, 0.32, 0.58],
            TransparentBlendMode::Alpha,
        ),
        (
            [0.0, -0.55, -0.6],
            [1.0, 0.9, 0.16, 0.54],
            TransparentBlendMode::Additive,
        ),
        (
            [0.8, -0.55, -0.6],
            [0.18, 0.55, 1.0, 0.58],
            TransparentBlendMode::Alpha,
        ),
    ];
    let items = particles
        .into_iter()
        .enumerate()
        .map(
            |(particle_id, (position, color, blend))| ParticleRenderItem {
                emitter_entity: 1,
                particle_id: particle_id as u64,
                position,
                size: 0.58,
                rotation: particle_id as f32 * 0.23,
                color,
                texture: (particle_id == 2).then(|| texture.clone()).flatten(),
                blend,
            },
        )
        .collect::<Vec<_>>();
    Arc::new(ParticleFrame {
        frame_id: 0,
        scene_generation: 0,
        items: Arc::from(items),
        dropped_count: 0,
        state_hash: [0; 32],
        status: FrameStatus::Ready,
    })
}

#[cfg(test)]
mod tests {
    use super::billboard_frame;
    use aether_engine::asset::{texture::CpuTexture, Handle};

    #[test]
    fn billboard_fixture_attaches_typed_texture_to_one_item() {
        let texture = Handle::<CpuTexture>::new(73);
        let frame = billboard_frame(Some(texture));
        assert_eq!(frame.items[2].texture, Some(Handle::<CpuTexture>::new(73)));
        assert_eq!(
            frame
                .items
                .iter()
                .filter(|item| item.texture.is_some())
                .count(),
            1
        );
    }
}
