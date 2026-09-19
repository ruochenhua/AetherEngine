// WGSL shader for deferred lighting.

pub(super) const LIGHTING_SHADER_SRC: &str = r#"
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

override ssao_enabled: bool = true;
override shadow_enabled: bool = true;
override ibl_enabled: bool = true;

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
    cascade_view_projs: array<mat4x4<f32>, 4>,
    cascade_splits: vec4<f32>,
    cascade_count: u32,
    inv_view_proj: mat4x4<f32>,
    camera_forward: vec3<f32>,
    _pad_cam: u32,
    ssao_enabled: u32,
    shadow_enabled: u32,
    ibl_enabled: u32,
    _pad4: u32,
};

struct GpuLocalLight {
    position_range: vec4<f32>,
    color_intensity: vec4<f32>,
    direction_inner_cos: vec4<f32>,
    outer_cos_type_pad: vec4<u32>,
};

struct LocalLightParams {
    count: u32,
    _pad: vec3<u32>,
};

@group(0) @binding(0) var gbuffer_position: texture_2d<f32>;
@group(0) @binding(1) var gbuffer_normal: texture_2d<f32>;
@group(0) @binding(2) var gbuffer_albedo: texture_2d<f32>;
@group(0) @binding(3) var gbuffer_material: texture_2d<f32>;
@group(0) @binding(4) var gbuffer_sampler: sampler;

@group(1) @binding(0) var<uniform> uniforms: LightingUniforms;

@group(2) @binding(0) var shadow_depth: texture_depth_2d_array;
@group(2) @binding(1) var shadow_sampler: sampler_comparison;
@group(2) @binding(2) var shadow_debug_sampler: sampler;

@group(3) @binding(0) var irradiance_map: texture_cube<f32>;
@group(3) @binding(1) var prefiltered_map: texture_cube<f32>;
@group(3) @binding(2) var brdf_lut: texture_2d<f32>;
@group(3) @binding(3) var ibl_sampler: sampler;
@group(3) @binding(4) var env_map: texture_cube<f32>;

@group(0) @binding(5) var ao_texture: texture_2d<f32>;

@group(3) @binding(5) var<storage, read> local_lights: array<GpuLocalLight>;
@group(3) @binding(6) var<uniform> local_light_params: LocalLightParams;

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

fn evaluate_direct_light(
    N: vec3<f32>,
    V: vec3<f32>,
    albedo: vec3<f32>,
    roughness: f32,
    metallic: f32,
    L: vec3<f32>,
    radiance: vec3<f32>,
) -> vec3<f32> {
    let F0 = mix(vec3<f32>(0.04), albedo, metallic);
    let H = normalize(L + V);
    let NdotL = max(dot(N, L), 0.0);
    let NdotV = max(dot(N, V), 0.0);
    let NdotH = max(dot(N, H), 0.0);
    let VdotH = max(dot(V, H), 0.0);
    let NDF = distribution_ggx(NdotH, roughness);
    let G = geometry_smith(NdotV, NdotL, roughness);
    let F = fresnel_schlick(VdotH, F0);
    let numerator = NDF * G * F;
    let denominator = 4.0 * NdotV * NdotL + 0.0001;
    let specular = numerator / denominator;
    let kS = F;
    let kD = (vec3<f32>(1.0) - kS) * (1.0 - metallic);
    let diffuse = kD * albedo / PI * NdotL * radiance;
    let specular_direct = specular * NdotL * radiance;
    return diffuse + specular_direct;
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
    } else if (albedo_sample.a > 0.5) {
        // Unlit scene markers are encoded in GBuffer albedo alpha. They are
        // intentionally unaffected by IBL, direct light, SSAO, or shadows.
        output_color = albedo_sample.rgb;
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

    // AO darkens both the constant ambient term and the IBL contribution.
    // When ssao_enabled=false, the override compiler eliminates the texture sample.
    var ao: f32 = 1.0;
    if (ssao_enabled) {
        ao = textureSample(ao_texture, gbuffer_sampler, in.uv).r;
    }
    let ambient = albedo * uniforms.ambient_intensity * ao;
    let radiance = uniforms.light.color * uniforms.light.intensity;
    let diffuse_direct = kD * albedo / PI * NdotL * radiance;
    let specular_direct = specular * NdotL * radiance;

    let lit_color = ambient + diffuse_direct + specular_direct;

    // Shadow: when shadow_enabled=false, the override compiler eliminates
    // the entire cascade selection + PCF 3x3 loop.
    var shadow_factor: f32 = 1.0;
    if (shadow_enabled) {
        let view_depth = dot(world_pos - uniforms.camera_pos, uniforms.camera_forward);
        // Default to last cascade for objects beyond the final split.
        var cascade_index: u32 = uniforms.cascade_count - 1u;
        for (var i: u32 = 0u; i < uniforms.cascade_count; i = i + 1u) {
            if (view_depth < uniforms.cascade_splits[i]) {
                cascade_index = i;
                break;
            }
        }

        let normal_offset = 0.03;
        let shadow_sample_pos = world_pos + N * normal_offset;
        let light_clip = uniforms.cascade_view_projs[cascade_index] * vec4<f32>(shadow_sample_pos, 1.0);
        var visibility: f32 = 1.0;
        if (light_clip.w > 0.0) {
            let light_ndc = light_clip.xyz / light_clip.w;
            let uv = vec2<f32>(light_ndc.x * 0.5 + 0.5, 0.5 - light_ndc.y * 0.5);
            let in_bounds = all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0))
                         && light_ndc.z >= 0.0 && light_ndc.z <= 1.0;
            if (in_bounds) {
                let cos_theta = saturate(NdotL);
                let sin_theta = sqrt(1.0 - cos_theta * cos_theta);
                let slope_bias = uniforms.shadow_normal_bias * sin_theta / max(cos_theta, 0.001);
                let bias = min(slope_bias, uniforms.shadow_normal_bias * 5.0);
                let ref_depth = light_ndc.z - bias;
                let texel_size = 1.0 / uniforms.shadow_map_size;
                visibility = 0.0;
                for (var x: i32 = -1; x <= 1; x = x + 1) {
                    for (var y: i32 = -1; y <= 1; y = y + 1) {
                        let offset = vec2<f32>(f32(x) * texel_size, f32(y) * texel_size);
                        visibility = visibility + textureSampleCompare(
                            shadow_depth, shadow_sampler, uv + offset, i32(cascade_index), ref_depth
                        );
                    }
                }
                visibility = visibility / 9.0;
            }
        }
        shadow_factor = mix(0.3, 1.0, visibility);
    }
    let direct_light = lit_color * shadow_factor;

    // Local point/spot lights are evaluated without shadowing in T2. Their
    // deterministic order and count are supplied by the extract phase.
    var local_direct = vec3<f32>(0.0);
    for (var local_index: u32 = 0u; local_index < local_light_params.count; local_index = local_index + 1u) {
        let local = local_lights[local_index];
        let to_light = local.position_range.xyz - world_pos;
        let distance_squared = max(dot(to_light, to_light), 0.0001);
        let distance_to_light = sqrt(distance_squared);
        if (distance_to_light < local.position_range.w) {
            let L_local = to_light / distance_to_light;
            let range_weight = 1.0 - distance_to_light / local.position_range.w;
            var cone_weight = 1.0;
            if (local.outer_cos_type_pad.y == 1u) {
                let surface_direction = normalize(world_pos - local.position_range.xyz);
                let cos_theta = dot(normalize(local.direction_inner_cos.xyz), surface_direction);
                let outer_cos = bitcast<f32>(local.outer_cos_type_pad.x);
                cone_weight = smoothstep(outer_cos, local.direction_inner_cos.w, cos_theta);
            }
            let attenuation = range_weight * range_weight / distance_squared;
            let radiance_local = local.color_intensity.rgb * local.color_intensity.w * attenuation * cone_weight;
            local_direct = local_direct + evaluate_direct_light(
                N, V, albedo, roughness, metallic, L_local, radiance_local
            );
        }
    }
    let direct_with_locals = direct_light + local_direct;

    // IBL (Image-Based Lighting) — uses same F (Fresnel) and kD as direct light
    let R = reflect(-V, N);

    // When ibl_enabled=false, the override compiler eliminates all cubemap samples.
    var ibl_light: vec3<f32> = vec3(0.0);
    if (ibl_enabled) {
        let irradiance = textureSample(irradiance_map, ibl_sampler, N).rgb;
        let diffuse_ibl = kD * albedo * irradiance;

        let mip_level = roughness * 4.0;
        let prefiltered_color = textureSampleLevel(prefiltered_map, ibl_sampler, vec3<f32>(R.x, -R.y, R.z), mip_level).rgb;
        let env_brdf = textureSample(brdf_lut, ibl_sampler, vec2<f32>(NdotV, roughness)).rg;
        let specular_ibl = prefiltered_color * (F * env_brdf.r + env_brdf.g);
        ibl_light = diffuse_ibl + specular_ibl;
    }

    let final_color = direct_with_locals + ibl_light * ao;

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
        // Shadow map as seen from light: sample depth at screen UV (cascade 0)
        let d = textureSample(shadow_depth, shadow_debug_sampler, in.uv, 0);
        output_color = vec3<f32>(d);
    } else if (uniforms.debug_mode == 7u) {
        // Direct lighting only (no IBL)
        output_color = direct_with_locals;
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
    } else if (uniforms.debug_mode == 15u) {
        // CSM cascade visualization: color-coded by cascade index
        let cascade_depth = dot(world_pos - uniforms.camera_pos, uniforms.camera_forward);
        var ci: u32 = uniforms.cascade_count - 1u;
        for (var j: u32 = 0u; j < uniforms.cascade_count; j = j + 1u) {
            if (cascade_depth < uniforms.cascade_splits[j]) {
                ci = j;
                break;
            }
        }
        // Color code: 0=red, 1=green, 2=blue, 3=yellow, 4+=magenta
        let c_norm = f32(ci) / f32(uniforms.cascade_count - 1u);
        if (ci == 0u) { output_color = vec3<f32>(1.0, 0.2, 0.2); }
        else if (ci == 1u) { output_color = vec3<f32>(0.2, 1.0, 0.2); }
        else if (ci == 2u) { output_color = vec3<f32>(0.2, 0.5, 1.0); }
        else { output_color = vec3<f32>(1.0, 1.0, 0.2); }
    } else {
        output_color = final_color;
    }

    }  // closes geometry else block

    // Return linear HDR — tone mapping is handled by ToneMappingPass
    return vec4<f32>(output_color, 1.0);
}
"#;
