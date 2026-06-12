//! Lighting Pass
//!
//! Full-screen quad pass that reads G-Buffer textures and computes
//! PBR lighting (Cook-Torrance BRDF with IBL). Outputs linear HDR
//! color to `SceneColor` (Rgba16Float) for downstream tone mapping.
//!
//! Implements the `Pass` trait for type-safe scheduling.

use crate::renderer::frame::RenderFrame;
use crate::renderer::light::LightingUniforms;
use crate::renderer::pass::{Pass, PassSignature, ResHandle};
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;
use wgpu::util::DeviceExt;

/// Lighting Pass implementation.
pub struct LightingPass {
    pipeline: wgpu::RenderPipeline,
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
    /// Feature toggle: SSAO enabled.
    ssao_enabled: bool,
    /// Feature toggle: shadow mapping enabled.
    shadow_enabled: bool,
    /// Feature toggle: IBL enabled.
    ibl_enabled: bool,
}

impl Pass for LightingPass {
    fn name(&self) -> &str {
        "Lighting"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("Lighting")
            .read::<GPosition>("gbuffer_position")
            .read::<GNormal>("gbuffer_normal")
            .read::<GAlbedo>("gbuffer_albedo")
            .read::<GMaterial>("gbuffer_material")
            .read::<ShadowDepth>("shadow_depth")
            .read::<AOTextureBlurred>("ao_blurred")
            .write::<SceneColor>("scene_color", wgpu::TextureFormat::Rgba16Float)
    }

    fn init(device: &wgpu::Device) -> Self {
        // Default format — will be set properly via new().
        let placeholder = Self::create_placeholder_ibl(device);
        Self::new_inner(device, wgpu::TextureFormat::Bgra8UnormSrgb, &placeholder)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.pos_handle = Some(resources.handle::<GPosition>("gbuffer_position"));
        self.normal_handle = Some(resources.handle::<GNormal>("gbuffer_normal"));
        self.albedo_handle = Some(resources.handle::<GAlbedo>("gbuffer_albedo"));
        self.material_handle = Some(resources.handle::<GMaterial>("gbuffer_material"));
        self.shadow_depth_handle = Some(resources.handle::<ShadowDepth>("shadow_depth"));
        self.ao_handle = Some(resources.handle::<AOTextureBlurred>("ao_blurred"));

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

        self.texture_bind_group = Some(device.create_bind_group(
            &wgpu::BindGroupDescriptor {
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
            },
        ));

        self.shadow_bind_group = Some(device.create_bind_group(
            &wgpu::BindGroupDescriptor {
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
            },
        ));

    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        let light_dir =
            glam::Vec3::from_array(frame.lighting.light.direction).normalize();
        let light_view_proj =
            crate::renderer::passes::shadow::compute_light_space_matrix(&light_dir);

        let mut uniforms = *frame.lighting;
        uniforms.camera_pos = frame.camera.position.into();
        uniforms.debug_mode = self.debug_mode;
        uniforms.ssao_enabled = if self.ssao_enabled { 1 } else { 0 };
        uniforms.shadow_enabled = if self.shadow_enabled { 1 } else { 0 };
        uniforms.ibl_enabled = if self.ibl_enabled { 1 } else { 0 };
        uniforms.shadow_map_size = crate::renderer::passes::shadow::SHADOW_MAP_SIZE as f32;
        uniforms.light_view_proj = light_view_proj.to_cols_array_2d();
        let proj = frame.camera.projection_matrix(frame.aspect);
        let view = frame.camera.view_matrix();
        let inv_view_proj = (proj * view).inverse();
        uniforms.inv_view_proj = inv_view_proj.to_cols_array_2d();
        frame.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[uniforms]),
        );
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        _surface_view: &wgpu::TextureView,
    ) {
        let texture_bg = self.texture_bind_group.as_ref()
            .expect("LightingPass: resolve not called");
        let scene_color_view = resources.get(resources.handle::<SceneColor>("scene_color"));

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

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, texture_bg, &[]);
        pass.set_bind_group(1, &self.uniform_bind_group, &[]);
        pass.set_bind_group(2, self.shadow_bind_group.as_ref().expect("Shadow BG not set"), &[]);
        pass.set_bind_group(3, &self.ibl_bind_group, &[]);
        pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
        pass.draw(0..self.quad_vertex_count, 0..1);
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl LightingPass {
    /// Create a new lighting pass with the given surface format.
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let placeholder = Self::create_placeholder_ibl(device);
        Self::new_inner(device, surface_format, &placeholder)
    }

    /// Create a lighting pass with custom IBL resources.
    pub fn new_with_ibl(device: &wgpu::Device, surface_format: wgpu::TextureFormat, ibl: &crate::renderer::ibl::IblResources) -> Self {
        Self::new_inner(device, surface_format, ibl)
    }

    fn new_inner(device: &wgpu::Device, _surface_format: wgpu::TextureFormat, ibl: &crate::renderer::ibl::IblResources) -> Self {
        // LightingPass outputs to SceneColor (Rgba16Float), not the swapchain
        let output_format = wgpu::TextureFormat::Rgba16Float;
        let shader_source = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@location(0) position: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(position, 0.0, 1.0);
    out.uv = vec2<f32>(position.x * 0.5 + 0.5, 0.5 - position.y * 0.5);
    return out;
}

struct DirectionalLight {
    direction: vec3<f32>,
    _pad: f32,
    color: vec3<f32>,
    intensity: f32,
};

struct LightingUniforms {
    camera_pos: vec3<f32>,
    _pad1: f32,
    light: DirectionalLight,
    ambient_intensity: f32,
    debug_mode: u32,
    shadow_normal_bias: f32,
    shadow_map_size: f32,
    light_view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    ssao_enabled: u32,
    shadow_enabled: u32,
    ibl_enabled: u32,
    _pad4: u32,
};

@group(0) @binding(0) var gbuffer_position: texture_2d<f32>;
@group(0) @binding(1) var gbuffer_normal: texture_2d<f32>;
@group(0) @binding(2) var gbuffer_albedo: texture_2d<f32>;
@group(0) @binding(3) var gbuffer_material: texture_2d<f32>;
@group(0) @binding(4) var gbuffer_sampler: sampler;

@group(1) @binding(0) var<uniform> uniforms: LightingUniforms;

@group(2) @binding(0) var shadow_depth: texture_depth_2d;
@group(2) @binding(1) var shadow_sampler: sampler_comparison;
@group(2) @binding(2) var shadow_debug_sampler: sampler;

@group(3) @binding(0) var irradiance_map: texture_cube<f32>;
@group(3) @binding(1) var prefiltered_map: texture_cube<f32>;
@group(3) @binding(2) var brdf_lut: texture_2d<f32>;
@group(3) @binding(3) var ibl_sampler: sampler;
@group(3) @binding(4) var env_map: texture_cube<f32>;

@group(0) @binding(5) var ao_texture: texture_2d<f32>;

// ── Cook-Torrance BRDF ──────────────────────────────────────────────

const PI: f32 = 3.14159265359;

fn distribution_ggx(NdotH: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let denom = NdotH * NdotH * (a2 - 1.0) + 1.0;
    return a2 / (PI * denom * denom);
}

fn geometry_schlick_ggx(NdotV: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return NdotV / (NdotV * (1.0 - k) + k);
}

fn geometry_smith(NdotV: f32, NdotL: f32, roughness: f32) -> f32 {
    let ggx2 = geometry_schlick_ggx(NdotV, roughness);
    let ggx1 = geometry_schlick_ggx(NdotL, roughness);
    return ggx1 * ggx2;
}

fn fresnel_schlick(cos_theta: f32, F0: vec3<f32>) -> vec3<f32> {
    return F0 + (vec3<f32>(1.0) - F0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let position_sample = textureSample(gbuffer_position, gbuffer_sampler, uv);
    let normal_sample = textureSample(gbuffer_normal, gbuffer_sampler, uv);
    let albedo_sample = textureSample(gbuffer_albedo, gbuffer_sampler, uv);
    let material_sample = textureSample(gbuffer_material, gbuffer_sampler, uv);

    let world_pos = position_sample.xyz;

    // Reconstruct view ray once for sky + debug modes
    let clip = vec4<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0, 0.0, 1.0);
    let world_ray = uniforms.inv_view_proj * clip;
    let world_pos_rc = world_ray.xyz / world_ray.w;
    let view_dir = normalize(world_pos_rc - uniforms.camera_pos);

    // Debug modes that override everything (no tone mapping for raw values)
    if (uniforms.debug_mode == 11u) {
        // NDC coordinates as RGB (no matrix — verifies UV→NDC reconstruction)
        let test = vec3<f32>(clip.xy * 0.5 + 0.5, 0.0);
        return vec4<f32>(test, 1.0);
    }
    if (uniforms.debug_mode == 12u) {
        // Raw env_map sample at FIXED forward direction — bypasses inv_view_proj
        let env_fixed = textureSampleLevel(env_map, ibl_sampler, vec3<f32>(0.0, 0.0, -1.0), 0.0).rgb;
        return vec4<f32>(env_fixed, 1.0);
    }
    if (uniforms.debug_mode == 13u) {
        // view_dir as RGB — should change when camera rotates
        return vec4<f32>(view_dir * 0.5 + 0.5, 1.0);
    }

    // Sky check: G-Buffer normal is (0,0,0,0) after clear.
    // Geometry normal is (N*0.5+0.5, 1.0) → RGB never all zero.
    var output_color: vec3<f32>;
    if (normal_sample.r == 0.0 && normal_sample.g == 0.0 && normal_sample.b == 0.0) {
        output_color = textureSampleLevel(env_map, ibl_sampler, view_dir, 0.0).rgb;
    } else {
    let N = normalize(normal_sample.xyz * 2.0 - 1.0);
    let albedo = albedo_sample.rgb;
    let roughness = material_sample.r;
    let metallic = material_sample.g;

    // PBR material parameters
    let F0 = mix(vec3<f32>(0.04), albedo, metallic);

    let L = normalize(-uniforms.light.direction);
    let V = normalize(uniforms.camera_pos - world_pos);
    let H = normalize(L + V);

    let NdotL = max(dot(N, L), 0.0);
    let NdotV = max(dot(N, V), 0.0);
    let NdotH = max(dot(N, H), 0.0);
    let VdotH = max(dot(V, H), 0.0);

    // Cook-Torrance specular
    let NDF = distribution_ggx(NdotH, roughness);
    let G = geometry_smith(NdotV, NdotL, roughness);
    let F = fresnel_schlick(VdotH, F0);

    let numerator = NDF * G * F;
    let denominator = 4.0 * NdotV * NdotL + 0.0001;
    let specular = numerator / denominator;

    // Diffuse with energy conservation (Fresnel-based kD)
    let kS = F;
    let kD = (vec3<f32>(1.0) - kS) * (1.0 - metallic);

    let ambient = albedo * uniforms.ambient_intensity;
    let radiance = uniforms.light.color * uniforms.light.intensity;
    let diffuse_direct = kD * albedo / PI * NdotL * radiance;
    let specular_direct = specular * NdotL * radiance;

    let lit_color = ambient + diffuse_direct + specular_direct;

    // Shadow: transform world_pos to light space, sample shadow map with
    // slope-scale depth bias (OpenGL Tutorial 16 approach):
    //   bias = base * tan(acos(NdotL)) = base * sqrt(1-NdotL²) / NdotL
    // Grazing angles get more bias; front-facing gets less. Clamped to avoid
    // peter panning on flat surfaces.
    //
    // We also apply a small world-space normal offset before projecting to
    // light space. This shifts the sampling position slightly along the
    // surface normal, which effectively moves receiver surfaces toward the
    // occluder in light-space and eliminates the gap (peter-panning)
    // without needing excessive depth bias.
    let normal_offset = 0.03;
    let shadow_sample_pos = world_pos + N * normal_offset;
    let light_clip = uniforms.light_view_proj * vec4<f32>(shadow_sample_pos, 1.0);
    var visibility: f32 = 1.0;
    if (light_clip.w > 0.0) {
        let light_ndc = light_clip.xyz / light_clip.w;
        let uv = vec2<f32>(light_ndc.x * 0.5 + 0.5, 0.5 - light_ndc.y * 0.5);
        // Bounds check: only sample shadow map when NDC is inside [0,1] cube.
        // Outside the frustum we assume fully lit (visibility = 1.0).
        let in_bounds = all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0))
                     && light_ndc.z >= 0.0 && light_ndc.z <= 1.0;
        if (in_bounds) {
            // Slope-scale bias: tan(acos(NdotL)) = sin(theta)/cos(theta)
            let cos_theta = saturate(NdotL);
            let sin_theta = sqrt(1.0 - cos_theta * cos_theta);
            let slope_bias = uniforms.shadow_normal_bias * sin_theta / max(cos_theta, 0.001);
            let bias = min(slope_bias, uniforms.shadow_normal_bias * 5.0);
            let ref_depth = light_ndc.z - bias;
            // PCF 3x3
            let texel_size = 1.0 / uniforms.shadow_map_size;
            visibility = 0.0;
            for (var x: i32 = -1; x <= 1; x = x + 1) {
                for (var y: i32 = -1; y <= 1; y = y + 1) {
                    let offset = vec2<f32>(f32(x) * texel_size, f32(y) * texel_size);
                    visibility = visibility + textureSampleCompare(
                        shadow_depth, shadow_sampler, uv + offset, ref_depth
                    );
                }
            }
            visibility = visibility / 9.0;
        }
    }
    let shadow_factor = select(1.0, mix(0.3, 1.0, visibility), uniforms.shadow_enabled != 0u);
    let direct_light = lit_color * shadow_factor;

    // IBL (Image-Based Lighting) — uses same F (Fresnel) and kD as direct light
    let R = reflect(-V, N);

    // Diffuse IBL: sample irradiance cubemap
    let irradiance = textureSample(irradiance_map, ibl_sampler, N).rgb;
    let diffuse_ibl = kD * albedo * irradiance;

    // Specular IBL: sample prefiltered cubemap + BRDF LUT
    let mip_level = roughness * 4.0; // 5 mips, mip 4 = roughness 1.0
    let prefiltered_color = textureSampleLevel(prefiltered_map, ibl_sampler, vec3<f32>(R.x, -R.y, R.z), mip_level).rgb;
    let env_brdf = textureSample(brdf_lut, ibl_sampler, vec2<f32>(NdotV, roughness)).rg;
    let specular_ibl = prefiltered_color * (F * env_brdf.r + env_brdf.g);

    let ibl_light = select(vec3<f32>(0.0), diffuse_ibl + specular_ibl, uniforms.ibl_enabled != 0u);

    // Sample AO texture (R8Unorm → f32 in .r)
    let ao_raw = textureSample(ao_texture, gbuffer_sampler, in.uv).r;
    let ao = select(1.0, ao_raw, uniforms.ssao_enabled != 0u);
    let final_color = direct_light + ibl_light * ao;

    if (uniforms.debug_mode == 1u) {
        output_color = ambient;
    } else if (uniforms.debug_mode == 2u) {
        output_color = diffuse_direct;
    } else if (uniforms.debug_mode == 3u) {
        output_color = specular_direct;
    } else if (uniforms.debug_mode == 4u) {
        output_color = N * 0.5 + 0.5;
    } else if (uniforms.debug_mode == 5u) {
        output_color = vec3<f32>(NdotL);
    } else if (uniforms.debug_mode == 6u) {
        // Shadow map as seen from light: sample depth at screen UV
        let d = textureSample(shadow_depth, shadow_debug_sampler, in.uv);
        output_color = vec3<f32>(d);
    } else if (uniforms.debug_mode == 7u) {
        // Direct lighting only (no IBL)
        output_color = direct_light;
    } else if (uniforms.debug_mode == 8u) {
        // IBL only (no direct)
        output_color = ibl_light;
    } else if (uniforms.debug_mode == 9u) {
        // Show position alpha (sky flag in GPosition): white=geometry, black=sky
        output_color = vec3<f32>(position_sample.a);
    } else if (uniforms.debug_mode == 10u) {
        // Show normal alpha for comparison
        output_color = vec3<f32>(normal_sample.a);
    } else if (uniforms.debug_mode == 14u) {
        // SSAO only visualization
        output_color = vec3<f32>(ao);
    } else {
        output_color = final_color;
    }

    }  // closes geometry else block

    // Return linear HDR — tone mapping is handled by ToneMappingPass
    return vec4<f32>(output_color, 1.0);
}
"#;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Lighting Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source)),
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Lighting Texture Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        let shadow_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Shadow Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Lighting Uniform Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let ibl_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("IBL Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::Cube,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::Cube,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::Cube,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Lighting Pipeline Layout"),
            bind_group_layouts: &[Some(&texture_bind_group_layout), Some(&uniform_bind_group_layout), Some(&shadow_bind_group_layout), Some(&ibl_bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Lighting Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x2,
                    }],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: output_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Lighting Uniform Buffer"),
            size: std::mem::size_of::<LightingUniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Lighting Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Full-screen quad vertices (2 triangles)
        let quad_vertices: [[f32; 2]; 6] = [
            [-1.0, -1.0],
            [1.0, -1.0],
            [1.0, 1.0],
            [-1.0, -1.0],
            [1.0, 1.0],
            [-1.0, 1.0],
        ];

        let quad_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Lighting Quad Vertex Buffer"),
            contents: bytemuck::cast_slice(&quad_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            pipeline,
            uniform_buffer,
            quad_vertex_buffer,
            quad_vertex_count: 6,
            pos_handle: None,
            normal_handle: None,
            albedo_handle: None,
            material_handle: None,
            shadow_depth_handle: None,
            ao_handle: None,
            texture_bind_group: None,
            shadow_bind_group: None,
            uniform_bind_group,
            texture_bind_group_layout,
            shadow_bind_group_layout,
            uniform_bind_group_layout,
            ibl_bind_group: device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("IBL Bind Group"),
                layout: &ibl_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&ibl.irradiance_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&ibl.prefiltered_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&ibl.brdf_lut_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&ibl.ibl_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(&ibl.env_view),
                    },
                ],
            }),
            debug_mode: 0,
            ssao_enabled: true,
            shadow_enabled: true,
            ibl_enabled: true,
        }
    }

    /// Create placeholder IBL resources (white environment) when no HDR is loaded.
    fn create_placeholder_ibl(device: &wgpu::Device) -> crate::renderer::ibl::IblResources {
        let config = crate::renderer::ibl::IblConfig::default();
        crate::renderer::ibl::IblResources::generate(device, None, &config)
    }

    /// Set debug visualization mode for the next frame.
    pub fn set_debug_mode(&mut self, mode: u32) {
        self.debug_mode = mode;
    }

    /// Toggle SSAO in the lighting shader.
    pub fn set_ssao_enabled(&mut self, enabled: bool) {
        self.ssao_enabled = enabled;
    }

    /// Toggle shadow mapping in the lighting shader.
    pub fn set_shadow_enabled(&mut self, enabled: bool) {
        self.shadow_enabled = enabled;
    }

    /// Toggle IBL in the lighting shader.
    pub fn set_ibl_enabled(&mut self, enabled: bool) {
        self.ibl_enabled = enabled;
    }
}
