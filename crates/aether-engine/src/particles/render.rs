use super::{ParticleFrame, ParticleRenderItem};
use crate::asset::{texture::CpuTexture, Handle};
use crate::renderer::transparent::TransparentBlendMode;
use glam::{Mat4, Vec4};

/// Instance data shared by the CPU packer and billboard WGSL.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct ParticleGpuItem {
    pub position_size: [f32; 4],
    pub rotation_depth: [f32; 2],
    pub texture_slot: u32,
    pub blend: u32,
    pub color: [f32; 4],
    pub uv_rect: [f32; 4],
}

/// CPU-side association retained until the transparent pass binds its typed texture.
pub(crate) struct PackedParticleItem {
    pub gpu: ParticleGpuItem,
    pub texture: Option<Handle<CpuTexture>>,
}

/// Pack a published snapshot into finite, far-to-near camera-space instances.
pub(crate) fn pack_items(frame: &ParticleFrame, view: Mat4) -> Vec<PackedParticleItem> {
    let mut items: Vec<_> = frame
        .items
        .iter()
        .filter_map(|item| {
            let view_position =
                view * Vec4::new(item.position[0], item.position[1], item.position[2], 1.0);
            let depth = -view_position.z;
            let valid_item = view_position.is_finite()
                && depth.is_finite()
                && depth > 0.0
                && item.position.iter().all(|value| value.is_finite())
                && item.size.is_finite()
                && item.size > 0.0
                && item.rotation.is_finite()
                && item.color.iter().all(|value| value.is_finite());
            valid_item.then_some((
                depth,
                item.emitter_entity,
                item.particle_id,
                pack_item(item, depth),
            ))
        })
        .collect();
    items.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    items.into_iter().map(|(_, _, _, packed)| packed).collect()
}

fn pack_item(item: &ParticleRenderItem, depth: f32) -> PackedParticleItem {
    PackedParticleItem {
        texture: item.texture,
        gpu: ParticleGpuItem {
            position_size: [
                item.position[0],
                item.position[1],
                item.position[2],
                item.size,
            ],
            rotation_depth: [item.rotation, depth],
            texture_slot: u32::from(item.texture.is_some()),
            blend: u32::from(item.blend == TransparentBlendMode::Additive),
            color: item.color,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
        },
    }
}

/// The slot-zero white fallback is represented as a solid white texel in shader.
pub(crate) const PARTICLE_BILLBOARD_SHADER: &str = r#"
struct ViewProjUniform { view: mat4x4<f32>, proj: mat4x4<f32> };
@group(0) @binding(0) var<uniform> vp: ViewProjUniform;

struct ParticleItem {
    position_size: vec4<f32>,
    rotation_depth: vec2<f32>,
    texture_slot: u32,
    blend: u32,
    color: vec4<f32>,
    uv_rect: vec4<f32>,
};
@group(1) @binding(0) var<storage, read> particles: array<ParticleItem>;

@group(2) @binding(0) var particle_texture: texture_2d<f32>;
@group(2) @binding(1) var particle_sampler: sampler;

struct VertexOutput { @builtin(position) clip_position: vec4<f32>, @location(0) color: vec4<f32>, @location(1) uv: vec2<f32> };

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, @builtin(instance_index) instance_index: u32) -> VertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-0.5, -0.5), vec2<f32>(0.5, -0.5), vec2<f32>(0.5, 0.5),
        vec2<f32>(-0.5, -0.5), vec2<f32>(0.5, 0.5), vec2<f32>(-0.5, 0.5));
    let item = particles[instance_index];
    let angle = item.rotation_depth.x;
    let c = cos(angle);
    let s = sin(angle);
    let corner = corners[vertex_index];
    let rotated = vec2<f32>(c * corner.x - s * corner.y, s * corner.x + c * corner.y);
    let camera_right = vec3<f32>(vp.view[0][0], vp.view[1][0], vp.view[2][0]);
    let camera_up = vec3<f32>(vp.view[0][1], vp.view[1][1], vp.view[2][1]);
    let world = item.position_size.xyz + (camera_right * rotated.x + camera_up * rotated.y) * item.position_size.w;
    var out: VertexOutput;
    out.clip_position = vp.proj * vp.view * vec4<f32>(world, 1.0);
    out.color = item.color;
    out.uv = item.uv_rect.xy + (corner + vec2<f32>(0.5)) * item.uv_rect.zw;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(particle_texture, particle_sampler, in.uv);
    let alpha = clamp(in.color.a * texel.a, 0.0, 1.0);
    return vec4<f32>(in.color.rgb * texel.rgb * alpha, alpha);
}
"#;

#[cfg(test)]
mod tests {
    use super::{pack_items, ParticleGpuItem, PARTICLE_BILLBOARD_SHADER};
    use crate::asset::{texture::CpuTexture, Handle};
    use crate::particles::ParticleFrame;
    use crate::renderer::transparent::TransparentBlendMode;
    use glam::Mat4;
    use std::sync::Arc;

    fn item(id: u64, z: f32) -> crate::particles::ParticleRenderItem {
        crate::particles::ParticleRenderItem {
            emitter_entity: 1,
            particle_id: id,
            position: [0.0, 0.0, z],
            size: 1.0,
            rotation: 0.0,
            color: [1.0; 4],
            blend: TransparentBlendMode::Alpha,
            texture: None,
        }
    }

    fn frame(items: Vec<crate::particles::ParticleRenderItem>) -> ParticleFrame {
        ParticleFrame {
            frame_id: 1,
            scene_generation: 0,
            status: if items.is_empty() {
                crate::particles::FrameStatus::Empty
            } else {
                crate::particles::FrameStatus::Ready
            },
            items: Arc::from(items),
            dropped_count: 0,
            state_hash: [0; 32],
        }
    }

    #[test]
    fn particle_gpu_item_has_the_64_byte_shader_abi() {
        assert_eq!(std::mem::size_of::<ParticleGpuItem>(), 64);
        assert_eq!(std::mem::align_of::<ParticleGpuItem>(), 4);
    }

    #[test]
    fn packing_uses_camera_depth_far_to_near_and_white_texture_fallback() {
        let packed = pack_items(&frame(vec![item(1, -2.0), item(2, -8.0)]), Mat4::IDENTITY);
        assert_eq!(packed.len(), 2);
        assert_eq!(packed[0].gpu.rotation_depth[1], 8.0);
        assert_eq!(packed[1].gpu.rotation_depth[1], 2.0);
        assert_eq!(packed[0].gpu.texture_slot, 0);
        assert_eq!(packed[0].gpu.uv_rect, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(packed[0].texture, None);
    }

    #[test]
    fn texture_handle_remains_typed_and_attached_after_depth_sorting() {
        let far = crate::particles::ParticleRenderItem {
            texture: Some(Handle::<CpuTexture>::new(44)),
            ..item(1, -8.0)
        };
        let packed = pack_items(&frame(vec![item(2, -2.0), far]), Mat4::IDENTITY);
        assert_eq!(packed[0].texture, Some(Handle::<CpuTexture>::new(44)));
        assert_eq!(packed[0].gpu.texture_slot, 1);
    }

    #[test]
    fn empty_particle_frame_packs_to_no_instances() {
        assert!(pack_items(&frame(Vec::new()), Mat4::IDENTITY).is_empty());
    }

    #[test]
    fn packing_rejects_behind_camera_and_non_finite_items() {
        let invalid = crate::particles::ParticleRenderItem {
            position: [0.0, 0.0, f32::NAN],
            ..item(3, -2.0)
        };
        let packed = pack_items(&frame(vec![item(1, 2.0), invalid]), Mat4::IDENTITY);
        assert!(packed.is_empty());
    }

    #[test]
    fn billboard_shader_parses_as_wgsl() {
        assert!(PARTICLE_BILLBOARD_SHADER.contains("textureSample"));
        naga::front::wgsl::parse_str(PARTICLE_BILLBOARD_SHADER)
            .expect("particle billboard shader should remain valid WGSL");
    }
}
