//! WGSL shader sources for the bloom pass.

/// Vertex + fragment shader for the bright-region extraction pass.
pub const EXTRACT: &str = r#"
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

struct BloomUniforms {
    threshold: f32,
    intensity: f32,
    bloom_intensity: f32,
    enabled: u32,
};

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var tex_sampler: sampler;

@group(1) @binding(0) var<uniform> uniforms: BloomUniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (uniforms.enabled == 0u) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let hdr = textureSample(input_texture, tex_sampler, in.uv).rgb;
    let luminance = dot(hdr, vec3<f32>(0.2126, 0.7152, 0.0722));
    if (luminance > uniforms.threshold) {
        return vec4<f32>((hdr - uniforms.threshold) * uniforms.intensity, 1.0);
    }
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}
"#;

/// Shared 9-tap Gaussian blur shader used for both downsample and upsample passes.
pub const BLUR: &str = r#"
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

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var tex_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel_size = 1.0 / vec2<f32>(textureDimensions(input_texture));
    let uv = in.uv;

    // 9-tap (3×3) Gaussian approximation
    // Center weight 0.25, cross neighbors 0.125, corners 0.0625
    let center = textureSample(input_texture, tex_sampler, uv).rgb;
    let n = textureSample(input_texture, tex_sampler, uv + vec2( 0.0, -1.0) * texel_size).rgb;
    let s = textureSample(input_texture, tex_sampler, uv + vec2( 0.0,  1.0) * texel_size).rgb;
    let e = textureSample(input_texture, tex_sampler, uv + vec2( 1.0,  0.0) * texel_size).rgb;
    let w = textureSample(input_texture, tex_sampler, uv + vec2(-1.0,  0.0) * texel_size).rgb;
    let ne = textureSample(input_texture, tex_sampler, uv + vec2( 1.0, -1.0) * texel_size).rgb;
    let nw = textureSample(input_texture, tex_sampler, uv + vec2(-1.0, -1.0) * texel_size).rgb;
    let se = textureSample(input_texture, tex_sampler, uv + vec2( 1.0,  1.0) * texel_size).rgb;
    let sw = textureSample(input_texture, tex_sampler, uv + vec2(-1.0,  1.0) * texel_size).rgb;

    return vec4<f32>(center * 0.25 + (n + s + e + w) * 0.125 + (ne + nw + se + sw) * 0.0625, 1.0);
}
"#;

/// Vertex + fragment shader for the final bloom composite pass.
pub const COMPOSITE: &str = r#"
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

struct BloomUniforms {
    threshold: f32,
    intensity: f32,
    bloom_intensity: f32,
    enabled: u32,
};

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var bloom_texture: texture_2d<f32>;
@group(0) @binding(2) var tex_sampler: sampler;

@group(1) @binding(0) var<uniform> uniforms: BloomUniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let hdr = textureSample(input_texture, tex_sampler, in.uv).rgb;
    if (uniforms.enabled == 0u) {
        return vec4<f32>(hdr, 1.0);
    }
    let bloom = textureSample(bloom_texture, tex_sampler, in.uv).rgb;
    return vec4<f32>(hdr + bloom * uniforms.bloom_intensity, 1.0);
}
"#;
