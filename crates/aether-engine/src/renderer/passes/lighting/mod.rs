//! Lighting Pass
//!
//! Full-screen quad pass that reads G-Buffer textures and computes
//! PBR lighting (Cook-Torrance BRDF with IBL). Outputs linear HDR
//! color to `SceneColor` (Rgba16Float) for downstream tone mapping.
//!
//! Implements the `Pass` trait for type-safe scheduling.

use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{InitContext, Pass, PassSignature, ResHandle};
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;

pub(crate) mod pipeline;

/// Lighting Pass implementation.
pub struct LightingPass {
    /// 8 pipeline variants indexed by bitmask: bit0=ssao, bit1=shadow, bit2=ibl.
    pipelines: [wgpu::RenderPipeline; 8],
    uniform_buffer: wgpu::Buffer,
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,
    /// G-Buffer texture handles (populated by resolve).
    pos_handle: Option<ResHandle<GPosition>>,
    normal_handle: Option<ResHandle<GNormal>>,
    albedo_handle: Option<ResHandle<GAlbedo>>,
    material_handle: Option<ResHandle<GMaterial>>,
    /// Shadow depth handle (populated by resolve).
    shadow_depth_handle: Option<ResHandle<ShadowDepth>>,
    /// Texture bind group (recreated during resolve).
    texture_bind_group: Option<wgpu::BindGroup>,
    /// Shadow bind group (recreated during resolve).
    shadow_bind_group: Option<wgpu::BindGroup>,
    /// Uniform bind group.
    uniform_bind_group: wgpu::BindGroup,
    /// Bind group layouts (needed for recreate).
    texture_bind_group_layout: wgpu::BindGroupLayout,
    shadow_bind_group_layout: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    /// IBL bind group (created in constructor, always present).
    ibl_bind_group: wgpu::BindGroup,
    /// AO texture handle (populated by resolve).
    ao_handle: Option<ResHandle<AOTextureBlurred>>,
    /// Debug visualization mode (set by Launcher, used in apply_frame).
    debug_mode: u32,
    /// Current pipeline variant bitmask: bit0=ssao, bit1=shadow, bit2=ibl.
    current_key: u8,
}

impl Pass for LightingPass {
    fn name(&self) -> &str {
        "Lighting"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("Lighting")
            .read::<GPosition>()
            .read::<GNormal>()
            .read::<GAlbedo>()
            .read::<GMaterial>()
            .read::<ShadowDepth>()
            .read::<AOTextureBlurred>()
            .write::<SceneColor>(wgpu::TextureFormat::Rgba16Float)
    }

    fn init(ctx: &InitContext) -> Self {
        // Use provided IBL resources when available; otherwise fall back to a
        // placeholder so the pipeline can still be built in tests.
        let placeholder;
        let ibl = match ctx.ibl_resources {
            Some(ibl) => ibl,
            None => {
                placeholder = pipeline::create_placeholder_ibl(ctx.device);
                &placeholder
            }
        };
        Self::new_inner(ctx.device, ctx.surface_format, ibl)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.pos_handle = Some(resources.handle::<GPosition>());
        self.normal_handle = Some(resources.handle::<GNormal>());
        self.albedo_handle = Some(resources.handle::<GAlbedo>());
        self.material_handle = Some(resources.handle::<GMaterial>());
        self.shadow_depth_handle = Some(resources.handle::<ShadowDepth>());
        self.ao_handle = Some(resources.handle::<AOTextureBlurred>());

        // Create samplers
        let gbuffer_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("GBuffer Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Shadow Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::Less),
            ..Default::default()
        });

        let shadow_debug_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Shadow Debug Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let pos_view = resources.get(self.pos_handle.unwrap());
        let norm_view = resources.get(self.normal_handle.unwrap());
        let albedo_view = resources.get(self.albedo_handle.unwrap());
        let material_view = resources.get(self.material_handle.unwrap());
        let shadow_view = resources.get(self.shadow_depth_handle.unwrap());
        let ao_view = resources.get(self.ao_handle.unwrap());

        self.texture_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Lighting Texture Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(pos_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(norm_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(albedo_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(material_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&gbuffer_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(ao_view),
                },
            ],
        }));

        self.shadow_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Lighting Shadow Bind Group"),
            layout: &self.shadow_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&shadow_debug_sampler),
                },
            ],
        }));
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        self.set_debug_mode(frame.config.debug_mode);
        self.set_ssao_enabled(frame.config.ssao_enabled);
        self.set_shadow_enabled(frame.config.shadow_enabled);
        self.set_ibl_enabled(frame.config.ibl_enabled);

        let light_dir = glam::Vec3::from_array(frame.lighting.light.direction).normalize();

        let mut uniforms = *frame.lighting;
        uniforms.camera_pos = frame.camera.position.into();
        uniforms.camera_forward = frame.camera.forward().to_array();
        uniforms.debug_mode = self.debug_mode;
        uniforms.shadow_map_size = crate::renderer::passes::shadow::SHADOW_MAP_SIZE as f32;

        let cascades = crate::renderer::passes::shadow::compute_cascades(frame, &light_dir);
        for (i, cascade) in cascades.iter().enumerate() {
            uniforms.cascade_view_projs[i] = cascade.view_proj.to_cols_array_2d();
            uniforms.cascade_splits[i] = cascade.split_depth;
        }
        uniforms.cascade_count = crate::renderer::passes::shadow::CASCADE_COUNT as u32;

        let proj = frame.camera.projection_matrix(frame.aspect);
        let view = frame.camera.view_matrix();
        let inv_view_proj = (proj * view).inverse();
        uniforms.inv_view_proj = inv_view_proj.to_cols_array_2d();
        frame
            .queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        _surface_view: &wgpu::TextureView,
    ) {
        let texture_bg = self
            .texture_bind_group
            .as_ref()
            .expect("LightingPass: resolve not called");
        let scene_color_view = resources.get(resources.handle::<SceneColor>());

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Lighting Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: scene_color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        // Bitmask: bit0=ssao(weight1), bit1=shadow(weight2), bit2=ibl(weight4).
        // Pipeline vec index: ssao*4 + shadow*2 + ibl*1 (from loop order).
        // Convert: ssao (bit0→weight4), shadow (bit1→weight2), ibl (bit2→weight1).
        let key = self.current_key;
        let idx = ((key & 1) << 2)       // bit0 (ssao) → weight 4
                | (key & 2)               // bit1 (shadow) → weight 2
                | ((key >> 2) & 1); // bit2 (ibl) → weight 1
        pass.set_pipeline(&self.pipelines[idx as usize]);
        pass.set_bind_group(0, texture_bg, &[]);
        pass.set_bind_group(1, &self.uniform_bind_group, &[]);
        pass.set_bind_group(
            2,
            self.shadow_bind_group.as_ref().expect("Shadow BG not set"),
            &[],
        );
        pass.set_bind_group(3, &self.ibl_bind_group, &[]);
        pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
        pass.draw(0..self.quad_vertex_count, 0..1);
    }
}

impl LightingPass {
    /// Create a new lighting pass with the given surface format.
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let placeholder = pipeline::create_placeholder_ibl(device);
        Self::new_inner(device, surface_format, &placeholder)
    }

    /// Create a lighting pass with custom IBL resources.
    pub fn new_with_ibl(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        ibl: &crate::renderer::ibl::IblResources,
    ) -> Self {
        Self::new_inner(device, surface_format, ibl)
    }

    fn new_inner(
        device: &wgpu::Device,
        _surface_format: wgpu::TextureFormat,
        ibl: &crate::renderer::ibl::IblResources,
    ) -> Self {
        let objects = pipeline::build_lighting_pipeline(device, ibl);
        Self {
            pipelines: objects.pipelines,
            uniform_buffer: objects.uniform_buffer,
            quad_vertex_buffer: objects.quad_vertex_buffer,
            quad_vertex_count: objects.quad_vertex_count,
            pos_handle: None,
            normal_handle: None,
            albedo_handle: None,
            material_handle: None,
            shadow_depth_handle: None,
            ao_handle: None,
            texture_bind_group: None,
            shadow_bind_group: None,
            uniform_bind_group: objects.uniform_bind_group,
            texture_bind_group_layout: objects.texture_bind_group_layout,
            shadow_bind_group_layout: objects.shadow_bind_group_layout,
            uniform_bind_group_layout: objects.uniform_bind_group_layout,
            ibl_bind_group: objects.ibl_bind_group,
            debug_mode: 0,
            current_key: 0b111,
        }
    }

    /// Set debug visualization mode for the next frame.
    pub fn set_debug_mode(&mut self, mode: u32) {
        self.debug_mode = mode;
    }

    /// Toggle SSAO in the lighting shader (bit 0).
    pub fn set_ssao_enabled(&mut self, enabled: bool) {
        if enabled {
            self.current_key |= 0b001;
        } else {
            self.current_key &= !0b001;
        }
    }

    /// Toggle shadow mapping in the lighting shader (bit 1).
    pub fn set_shadow_enabled(&mut self, enabled: bool) {
        if enabled {
            self.current_key |= 0b010;
        } else {
            self.current_key &= !0b010;
        }
    }

    /// Toggle IBL in the lighting shader (bit 2).
    pub fn set_ibl_enabled(&mut self, enabled: bool) {
        if enabled {
            self.current_key |= 0b100;
        } else {
            self.current_key &= !0b100;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::headless_device_queue;
    use std::any::TypeId;

    #[test]
    fn signature_declares_gbuffer_shadow_ao_reads_and_scene_color_write() {
        let Some((device, _queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let sig = LightingPass::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb).signature();
        assert_eq!(sig.name, "Lighting");

        // 4 G-Buffer attachments + shadow depth + blurred AO.
        let expected_reads: [(TypeId, &'static str); 6] = [
            (TypeId::of::<GPosition>(), GPosition::NAME),
            (TypeId::of::<GNormal>(), GNormal::NAME),
            (TypeId::of::<GAlbedo>(), GAlbedo::NAME),
            (TypeId::of::<GMaterial>(), GMaterial::NAME),
            (TypeId::of::<ShadowDepth>(), ShadowDepth::NAME),
            (TypeId::of::<AOTextureBlurred>(), AOTextureBlurred::NAME),
        ];
        assert_eq!(
            sig.reads.len(),
            expected_reads.len(),
            "Lighting must read exactly the 4 G-Buffer textures + shadow map + blurred AO"
        );
        for (type_id, name) in expected_reads {
            assert!(
                sig.reads
                    .iter()
                    .any(|s| s.type_id == type_id && s.name == name),
                "missing read slot for {name}"
            );
        }

        assert_eq!(sig.writes.len(), 1, "Lighting writes exactly one target");
        assert_eq!(sig.writes[0].type_id, TypeId::of::<SceneColor>());
        assert_eq!(sig.writes[0].name, SceneColor::NAME);
        assert_eq!(
            sig.writes[0].format,
            Some(wgpu::TextureFormat::Rgba16Float),
            "SceneColor must stay linear HDR for the post-process chain"
        );
    }

    #[test]
    fn init_builds_all_eight_pipeline_variants() {
        let Some((device, _queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        // Compiles the inline WGSL and builds one pipeline per
        // ssao/shadow/ibl bitmask combination — must not panic.
        let pass = LightingPass::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb);
        assert_eq!(
            pass.pipelines.len(),
            8,
            "expected 8 pipeline variants (ssao × shadow × ibl bitmask)"
        );
    }
}
