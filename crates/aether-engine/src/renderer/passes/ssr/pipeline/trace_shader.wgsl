
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(pos, 0.0, 1.0);
    out.uv = vec2<f32>(pos.x * 0.5 + 0.5, 0.5 - pos.y * 0.5);
    return out;
}

struct SSRSettings {
    camera_pos: vec3<f32>,
    _pad0: f32,
    view_proj: mat4x4<f32>,
    screen_size: vec2<f32>,
    _pad1: vec2<f32>,
    max_distance: f32,
    linear_steps: f32,
    thickness: f32,
    step_exponent: f32,
    jitter_amount: f32,
    min_roughness: f32,
    max_roughness: f32,
    edge_fade_start: f32,
    edge_fade_end: f32,
    ssr_debug_mode: u32,
    ssr_enabled: u32,
    frame_index: u32,
    _pad2: u32,
    _pad3: u32,
    _pad4: u32,
    _pad5: u32,
};

@group(0) @binding(0) var gbuffer_position: texture_2d<f32>;
@group(0) @binding(1) var gbuffer_normal: texture_2d<f32>;
@group(0) @binding(2) var gbuffer_material: texture_2d<f32>;
@group(0) @binding(3) var gbuffer_depth: texture_depth_2d;
@group(0) @binding(4) var scene_color: texture_2d<f32>;
@group(0) @binding(5) var tex_sampler: sampler;

@group(1) @binding(0) var<uniform> settings: SSRSettings;

fn hash2(p: vec2<f32>) -> vec2<f32> {
    let h = fract(sin(vec2<f32>(dot(p, vec2<f32>(127.1, 311.7)), dot(p, vec2<f32>(269.5, 183.3)))) * 43758.5453);
    return h * 2.0 - 1.0;
}

// PCG 2D random number generator
fn pcg2d(v: vec2<u32>) -> vec2<u32> {
    var v_out = v;
    v_out.x = v_out.x * 747796405u + 2891336453u;
    v_out.y = v_out.y * 747796405u + 2891336453u;
    let word = ((v_out.x >> ((v_out.y >> 28u) + 4u)) ^ v_out.y) * 277803737u;
    v_out.x = (word >> 22u) ^ word;
    v_out.y = v_out.x;
    return v_out;
}

// Generate pseudo-random vec2 in [0,1] from screen pixel coord
fn rand2d(uv: vec2<f32>, frame: u32) -> vec2<f32> {
    let px = vec2<u32>(uv * settings.screen_size) ^ vec2<u32>(frame, frame * 3u);
    let h = pcg2d(px);
    return vec2<f32>(h) / 4294967295.0;
}

fn depth_intersects(
    previous_ray_depth: f32,
    current_ray_depth: f32,
    scene_depth: f32,
    thickness: f32,
) -> bool {
    let tolerance = max(thickness, 0.01);
    return abs(current_ray_depth - scene_depth) <= tolerance ||
        (previous_ray_depth > scene_depth + tolerance &&
            current_ray_depth <= scene_depth + tolerance) ||
        (previous_ray_depth < scene_depth - tolerance &&
            current_ray_depth >= scene_depth - tolerance);
}

// Sample the Visible Normal Distribution Function (VNDF) for GGX.
// Based on Heitz 2018 "Sampling the GGX Distribution of Visible Normals".
// V: view vector in tangent space (pointing toward surface)
// alpha: roughness squared
// u1, u2: random numbers in [0,1]
// Returns sampled half-vector H.
fn sample_vndf_ggx(V: vec3<f32>, alpha: f32, u1: f32, u2: f32) -> vec3<f32> {
    // Stretch view
    let Vh = normalize(vec3<f32>(alpha * V.x, alpha * V.y, V.z));

    // Build orthonormal basis around Vh
    let lensq = Vh.x * Vh.x + Vh.y * Vh.y;
    var T1: vec3<f32>;
    if (lensq > 0.0) {
        T1 = vec3<f32>(-Vh.y, Vh.x, 0.0) / sqrt(lensq);
    } else {
        T1 = vec3<f32>(1.0, 0.0, 0.0);
    }
    let T2 = cross(Vh, T1);

    // Sample point in disk
    let r = sqrt(u1);
    let phi = 2.0 * 3.14159265359 * u2;
    var t1 = r * cos(phi);
    var t2 = r * sin(phi);
    let s = 0.5 * (1.0 + Vh.z);
    t2 = mix(sqrt(max(0.0, 1.0 - t1 * t1)) * t2, s * t2, s);

    // Reproject onto hemisphere
    let Nh = t1 * T1 + t2 * T2 + sqrt(max(0.0, 1.0 - t1 * t1 - t2 * t2)) * Vh;

    // Unstretch and normalize
    return normalize(vec3<f32>(alpha * Nh.x, alpha * Nh.y, max(0.0, Nh.z)));
}

// Screen-space ray march following the article approach.
// Returns (hit, hit_uv.x, hit_uv.y, steps_taken_ratio).
fn ray_march(
    world_pos: vec3<f32>,
    rd: vec3<f32>,
    uv: vec2<f32>,
    roughness: f32,
) -> vec4<f32> {
    // Offset start point to avoid self-intersection without skipping nearby
    // reflected surfaces. The hit test already has a configurable thickness.
    let start_offset = max(0.02, settings.thickness * 0.1);
    let start_pos = world_pos + rd * start_offset;
    let end_pos = world_pos + rd * settings.max_distance;

    var start_clip = settings.view_proj * vec4<f32>(start_pos, 1.0);
    var end_clip = settings.view_proj * vec4<f32>(end_pos, 1.0);

    // Clip ray to camera front plane in clip space.
    // If both endpoints are behind the camera, the ray can't produce a valid reflection.
    let epsilon = 0.01;
    if (start_clip.w < epsilon && end_clip.w < epsilon) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    if (start_clip.w < epsilon) {
        let t = (epsilon - start_clip.w) / (end_clip.w - start_clip.w);
        start_clip = mix(start_clip, end_clip, t);
    }
    if (end_clip.w < epsilon) {
        let t = (epsilon - start_clip.w) / (end_clip.w - start_clip.w);
        end_clip = mix(start_clip, end_clip, t);
    }

    let start_ndc = start_clip.xyz / start_clip.w;
    let end_ndc = end_clip.xyz / end_clip.w;

    let ndc_delta = end_ndc - start_ndc;
    var t_min = 0.0;
    var t_max = 1.0;

    // X bounds [-1, 1]
    if (abs(ndc_delta.x) > 1e-6) {
        let tx1 = (-1.0 - start_ndc.x) / ndc_delta.x;
        let tx2 = (1.0 - start_ndc.x) / ndc_delta.x;
        t_min = max(t_min, min(tx1, tx2));
        t_max = min(t_max, max(tx1, tx2));
    } else if (start_ndc.x < -1.0 || start_ndc.x > 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // Y bounds [-1, 1]
    if (abs(ndc_delta.y) > 1e-6) {
        let ty1 = (-1.0 - start_ndc.y) / ndc_delta.y;
        let ty2 = (1.0 - start_ndc.y) / ndc_delta.y;
        t_min = max(t_min, min(ty1, ty2));
        t_max = min(t_max, max(ty1, ty2));
    } else if (start_ndc.y < -1.0 || start_ndc.y > 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // Z bounds [0, 1]
    if (abs(ndc_delta.z) > 1e-6) {
        let tz1 = (0.0 - start_ndc.z) / ndc_delta.z;
        let tz2 = (1.0 - start_ndc.z) / ndc_delta.z;
        t_min = max(t_min, min(tz1, tz2));
        t_max = min(t_max, max(tz1, tz2));
    } else if (start_ndc.z < 0.0 || start_ndc.z > 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    if (t_min > t_max) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    let clipped_start_ndc = start_ndc + ndc_delta * t_min;
    let clipped_end_ndc = start_ndc + ndc_delta * t_max;
    let clipped_start_pos = mix(start_pos, end_pos, t_min);
    let clipped_end_pos = mix(start_pos, end_pos, t_max);

    let start_screen = vec3<f32>(
        (clipped_start_ndc.x + 1.0) * 0.5 * settings.screen_size.x,
        (1.0 - clipped_start_ndc.y) * 0.5 * settings.screen_size.y,
        clipped_start_ndc.z
    );
    let end_screen = vec3<f32>(
        (clipped_end_ndc.x + 1.0) * 0.5 * settings.screen_size.x,
        (1.0 - clipped_end_ndc.y) * 0.5 * settings.screen_size.y,
        clipped_end_ndc.z
    );

    let screen_diff = end_screen - start_screen;
    let pixel_dist = max(abs(screen_diff.x), abs(screen_diff.y));
    // Keep a stable minimum quality while adding samples for long projected rays.
    // The previous implementation declared linear_steps but never used it.
    var sample_count = i32(settings.linear_steps);
    sample_count = clamp(sample_count, 4, 32);
    let projected_steps = i32(ceil(pixel_dist * 0.5));
    sample_count = max(sample_count, min(projected_steps, 32));

    var last_t = 0.0;
    let initial_ray_clip = settings.view_proj * vec4<f32>(clipped_start_pos, 1.0);
    var last_ray_depth = initial_ray_clip.w;
    var hit = 0.0;
    var hit_uv = vec2<f32>(0.0);
    var steps_taken = 0.0;

    let dims = vec2<i32>(settings.screen_size);
    let roughness_jitter = smoothstep(
        settings.min_roughness,
        settings.max_roughness,
        roughness,
    );
    let jitter_val = settings.jitter_amount * roughness_jitter *
        (rand2d(uv, settings.frame_index).x - 0.5);

    for (var i = 1; i <= sample_count; i++) {
        let raw_t = (f32(i) + jitter_val) / f32(sample_count);
        let current_t = pow(clamp(raw_t, 0.0, 1.0), settings.step_exponent);
        let ray_world_pos = mix(clipped_start_pos, clipped_end_pos, current_t);
        let ray_clip = settings.view_proj * vec4<f32>(ray_world_pos, 1.0);
        let ray_depth = ray_clip.w;
        let ray_ndc = ray_clip.xyz / ray_clip.w;
        let current_screen = vec3<f32>(
            (ray_ndc.x + 1.0) * 0.5 * settings.screen_size.x,
            (1.0 - ray_ndc.y) * 0.5 * settings.screen_size.y,
            ray_ndc.z,
        );
        steps_taken = f32(i) - 1.0;

        let px = vec2<i32>(current_screen.xy);
        let clamped_px = clamp(px, vec2<i32>(0), dims - vec2<i32>(1));

        let scene_pos = textureLoad(gbuffer_position, clamped_px, 0).xyz;

        if (length(scene_pos) < 0.0001) {
            last_ray_depth = ray_depth;
            last_t = current_t;
            continue;
        }

        let scene_clip = settings.view_proj * vec4<f32>(scene_pos, 1.0);
        if (depth_intersects(
            last_ray_depth,
            ray_depth,
            scene_clip.w,
            settings.thickness,
        )) {

            // Binary search refinement in screen-space t
            var t0 = last_t;
            var t1 = current_t;

            for (var b = 0; b < 4; b++) {
                let tm = (t0 + t1) * 0.5;
                let ray_world_m = mix(clipped_start_pos, clipped_end_pos, tm);
                let ray_clip_m = settings.view_proj * vec4<f32>(ray_world_m, 1.0);
                let ray_ndc_m = ray_clip_m.xyz / ray_clip_m.w;
                let sm = vec3<f32>(
                    (ray_ndc_m.x + 1.0) * 0.5 * settings.screen_size.x,
                    (1.0 - ray_ndc_m.y) * 0.5 * settings.screen_size.y,
                    ray_ndc_m.z,
                );
                let spx = vec2<i32>(sm.xy);
                let scl = clamp(spx, vec2<i32>(0), dims - vec2<i32>(1));
                let smp = textureLoad(gbuffer_position, scl, 0).xyz;

                if (length(smp) < 0.0001) {
                    t0 = tm;
                    continue;
                }

                let smc = settings.view_proj * vec4<f32>(smp, 1.0);
                let ray_world_0 = mix(clipped_start_pos, clipped_end_pos, t0);
                let ray_clip_0 = settings.view_proj * vec4<f32>(ray_world_0, 1.0);

                // Compare view-space depth (clip.w) instead of non-linear NDC-Z.
                if (depth_intersects(
                    ray_clip_0.w,
                    ray_clip_m.w,
                    smc.w,
                    settings.thickness,
                )) {
                    t1 = tm;
                } else {
                    t0 = tm;
                }
            }

            let final_t = (t0 + t1) * 0.5;
            let final_world_pos = mix(clipped_start_pos, clipped_end_pos, final_t);
            let final_clip = settings.view_proj * vec4<f32>(final_world_pos, 1.0);
            let final_ndc = final_clip.xyz / final_clip.w;
            let final_screen = vec3<f32>(
                (final_ndc.x + 1.0) * 0.5 * settings.screen_size.x,
                (1.0 - final_ndc.y) * 0.5 * settings.screen_size.y,
                final_ndc.z,
            );
            hit_uv = final_screen.xy / settings.screen_size;

            // Outlier rejection in the same linear depth space as the hit test.
            let hit_px = vec2<i32>(hit_uv * settings.screen_size);
            let hit_clamped_px = clamp(hit_px, vec2<i32>(0), dims - vec2<i32>(1));
            let hit_position = textureLoad(gbuffer_position, hit_clamped_px, 0).xyz;
            let hit_clip = settings.view_proj * vec4<f32>(hit_position, 1.0);
            if (abs(hit_clip.w - final_clip.w) <= settings.thickness * 2.0) {
                hit = 1.0;
                break;
            }
            // Inconsistent: treat as miss and keep marching
            hit = 0.0;
        }

        last_ray_depth = ray_depth;
        last_t = current_t;
    }

    return vec4<f32>(hit, hit_uv.x, hit_uv.y, steps_taken / f32(sample_count));
}

// Simple 2D hash for deterministic per-pixel noise (no temporal flicker)
fn hash22(p: vec2<f32>) -> vec2<f32> {
    var p3 = fract(vec3<f32>(p.xyx) * vec3<f32>(0.1031, 0.1030, 0.0973));
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.xx + p3.yz) * p3.zy);
}

// Screen-edge fade factor
fn edge_fade(uv: vec2<f32>) -> f32 {
    let edge_dist = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
    return smoothstep(settings.edge_fade_start, settings.edge_fade_end, edge_dist);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    // Trace is half-resolution, but the G-buffer remains full-resolution.
    // Load one stable source pixel instead of filtering across geometry edges.
    let source_dims = vec2<i32>(settings.screen_size);
    let source_px = clamp(
        vec2<i32>(uv * settings.screen_size),
        vec2<i32>(0),
        source_dims - vec2<i32>(1)
    );
    let pos_sample = textureLoad(gbuffer_position, source_px, 0);
    let norm_sample = textureLoad(gbuffer_normal, source_px, 0);
    let material_sample = textureLoad(gbuffer_material, source_px, 0);

    // SSR toggle
    if (settings.ssr_enabled == 0u) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // Sky check: GBuffer normal is (0,0,0) after clear
    if (norm_sample.r == 0.0 && norm_sample.g == 0.0 && norm_sample.b == 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    let world_pos = pos_sample.xyz;
    let N = normalize(norm_sample.xyz * 2.0 - 1.0);
    let roughness = material_sample.r;
    let metallic = material_sample.g;

    let view_dir = normalize(world_pos - settings.camera_pos);

    // Roughness fade: only process low-roughness surfaces
    let roughness_factor = 1.0 - smoothstep(settings.min_roughness, settings.max_roughness, roughness);

    if (roughness_factor <= 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // Keep mirror-like surfaces stable; rougher materials may still use a
    // small deterministic perturbation to avoid perfectly repeated stripes.
    let noise = hash22(uv * 43758.5453);
    let roughness_squared = roughness * roughness;
    let N_perturbed = normalize(N + vec3<f32>(noise.x - 0.5, noise.y - 0.5, 0.0) * roughness_squared * 0.5);
    let rd = normalize(reflect(view_dir, N_perturbed));

    // Debug modes from slice 2
    if (settings.ssr_debug_mode == 1u) {
        return vec4<f32>(rd * 0.5 + 0.5, 1.0);
    }
    if (settings.ssr_debug_mode == 2u) {
        return vec4<f32>(normalize(settings.camera_pos - world_pos) * 0.5 + 0.5, 1.0);
    }
    if (settings.ssr_debug_mode == 3u) {
        return vec4<f32>(vec3<f32>(roughness), 1.0);
    }
    if (settings.ssr_debug_mode == 4u) {
        return vec4<f32>(vec3<f32>(metallic), 1.0);
    }

    // Screen-space ray march
    let result = ray_march(world_pos, rd, uv, roughness);
    var hit = result.x;
    let hit_uv = result.yz;
    let step_norm = result.w;

    // Reject self-intersections using depth threshold.
    // If the hit point depth is close to the starting pixel depth,
    // it's likely the same surface, not a valid reflection.
    if (hit > 0.5) {
        let full_res = vec2<i32>(settings.screen_size);
        let hit_px = vec2<i32>(hit_uv * settings.screen_size);
        let hit_px_clamped = clamp(hit_px, vec2<i32>(0), full_res - vec2<i32>(1));
        let orig_px = vec2<i32>(uv * settings.screen_size);
        let orig_px_clamped = clamp(orig_px, vec2<i32>(0), full_res - vec2<i32>(1));
        let hit_position = textureLoad(gbuffer_position, hit_px_clamped, 0).xyz;
        let hit_clip = settings.view_proj * vec4<f32>(hit_position, 1.0);
        let orig_clip = settings.view_proj * vec4<f32>(world_pos, 1.0);
        if (abs(hit_clip.w - orig_clip.w) < settings.thickness * 0.5) {
            hit = 0.0;
        }
    }

    // Debug modes from slice 3
    if (settings.ssr_debug_mode == 5u) {
        // Hit UV visualization
        if (hit > 0.5) {
            return vec4<f32>(hit_uv.x, hit_uv.y, 0.0, 1.0);
        } else {
            return vec4<f32>(1.0, 0.0, 0.0, 1.0);
        }
    }
    if (settings.ssr_debug_mode == 6u) {
        // Hit/Miss binary
        let color = select(vec3<f32>(0.0), vec3<f32>(1.0), hit > 0.5);
        return vec4<f32>(color, 1.0);
    }
    if (settings.ssr_debug_mode == 7u) {
        // Step count heatmap
        let heat = step_norm;
        return vec4<f32>(heat, heat * 0.5, 1.0 - heat, 1.0);
    }
    if (settings.ssr_debug_mode == 8u) {
        // Ray endpoint UV
        let ray_end = world_pos + rd * settings.max_distance;
        let cs_end = settings.view_proj * vec4<f32>(ray_end, 1.0);
        let ndc_end = cs_end.xyz / cs_end.w;
        var uv_end = ndc_end.xy * 0.5 + 0.5;
        uv_end.y = 1.0 - uv_end.y;
        return vec4<f32>(uv_end.x, uv_end.y, 0.0, 1.0);
    }

    // Slice 4: color sampling and compositing
    if (hit > 0.5) {
        let edge_factor = edge_fade(hit_uv);
        let ssr_color = textureSample(scene_color, tex_sampler, hit_uv).rgb;
        // SSR is a single noisy screen-space sample. Use a perceptual
        // roughness falloff so medium/rough surfaces retain a soft fallback
        // while avoiding visible speckle from low-confidence hits.
        let alpha = hit * edge_factor * roughness_factor * roughness_factor;
        return vec4<f32>(ssr_color, alpha);
    }

    // No hit: output transparent
    return vec4<f32>(0.0, 0.0, 0.0, 0.0);
}
