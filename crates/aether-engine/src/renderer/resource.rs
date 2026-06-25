//! Resource type tags for type-safe transient resource handles.
//!
//! Each tag is a zero-size type. `ResHandle<T>` is parameterized on these tags,
//! so the compiler prevents passing one texture type where another is expected.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use aether_engine::renderer::resource::{GPosition, GNormal, AOTexture};
//! use aether_engine::renderer::pass::ResHandle;
//!
//! // These are different types — the compiler won't let you swap them.
//! fn read_position(handle: ResHandle<GPosition>) { /* ... */ }
//! fn read_ao(handle: ResHandle<AOTexture>) { /* ... */ }
//! ```

/// Trait for zero-size resource type tags.
pub trait ResourceTag: 'static {
    /// Logical name of this resource in the render graph.
    const NAME: &'static str;
}

/// Implement [`ResourceTag`] for a zero-size tag type.
macro_rules! impl_resource_tag {
    ($name:ident, $str:expr $(,)?) => {
        impl ResourceTag for $name {
            const NAME: &'static str = $str;
        }
    };
}

// ---------------------------------------------------------------------------
// G-Buffer tags
// ---------------------------------------------------------------------------

/// G-Buffer position texture (RGBA16Float).
pub enum GPosition {}
impl_resource_tag!(GPosition, "gbuffer_position");

/// G-Buffer normal texture (RGBA16Float).
pub enum GNormal {}
impl_resource_tag!(GNormal, "gbuffer_normal");

/// G-Buffer albedo texture (RGBA8Unorm).
pub enum GAlbedo {}
impl_resource_tag!(GAlbedo, "gbuffer_albedo");

/// G-Buffer material texture (RG8Unorm: roughness, metallic).
pub enum GMaterial {}
impl_resource_tag!(GMaterial, "gbuffer_material");

/// G-Buffer depth texture (Depth32Float).
pub enum GDepth {}
impl_resource_tag!(GDepth, "gbuffer_depth");

// ---------------------------------------------------------------------------
// Pipeline tags
// ---------------------------------------------------------------------------

/// Swapchain output texture.
pub enum Swapchain {}
impl_resource_tag!(Swapchain, "swapchain");

// ---------------------------------------------------------------------------
// Screen-space effect tags
// ---------------------------------------------------------------------------

/// Ambient occlusion texture (R8Unorm).
pub enum AOTexture {}
impl_resource_tag!(AOTexture, "ao");

/// Blurred ambient occlusion texture (R8Unorm).
pub enum AOTextureBlurred {}
impl_resource_tag!(AOTextureBlurred, "ao_blurred");

/// SSR ray-trace result texture (Rgba16Float).
pub enum SsrTraceResult {}
impl_resource_tag!(SsrTraceResult, "ssr_trace");

/// Irradiance cubemap (Rgba16Float, 32×32 per face).
pub enum IrradianceMap {}
impl_resource_tag!(IrradianceMap, "irradiance_map");

/// Prefiltered specular cubemap (Rgba16Float, 128×128 base, 5 mip levels).
pub enum PrefilteredMap {}
impl_resource_tag!(PrefilteredMap, "prefiltered_map");

/// BRDF integration LUT (Rgba16Float 2D, 256×256).
pub enum BrdfLUT {}
impl_resource_tag!(BrdfLUT, "brdf_lut");

/// Screen-space reflection texture (R11G11B10Float, half-resolution).
pub enum ReflectionTexture {}
impl_resource_tag!(ReflectionTexture, "reflection");

/// Scene color texture after deferred lighting (Rgba16Float).
pub enum SceneColor {}
impl_resource_tag!(SceneColor, "scene_color");

/// Water overlay color/alpha (Rgba16Float).
pub enum WaterColor {}
impl_resource_tag!(WaterColor, "water_color");

/// Volumetric cloud overlay color/alpha (Rgba16Float).
pub enum CloudColor {}
impl_resource_tag!(CloudColor, "cloud_color");

/// God ray light-shaft overlay (Rgba16Float).
pub enum GodRayColor {}
impl_resource_tag!(GodRayColor, "god_ray_color");

/// Post-process input texture — HDR linear output from composite (Rgba16Float).
pub enum PostProcessInput {}
impl_resource_tag!(PostProcessInput, "post_process_input");

/// Bright regions extracted for bloom (Rgba16Float).
pub enum BrightTexture {}
impl_resource_tag!(BrightTexture, "bright");

/// Bloom mip level 0 — half resolution (Rgba16Float).
pub enum BloomMip0 {}
impl_resource_tag!(BloomMip0, "bloom_mip0");

/// Bloom mip level 1 — quarter resolution (Rgba16Float).
pub enum BloomMip1 {}
impl_resource_tag!(BloomMip1, "bloom_mip1");

/// Bloom mip level 2 — eighth resolution (Rgba16Float).
pub enum BloomMip2 {}
impl_resource_tag!(BloomMip2, "bloom_mip2");

/// Final blurred bloom texture — full resolution (Rgba16Float).
pub enum BloomTexture {}
impl_resource_tag!(BloomTexture, "bloom_texture");

/// Bloom composited with HDR scene (Rgba16Float).
pub enum BloomResult {}
impl_resource_tag!(BloomResult, "bloom_result");

/// Tone-mapped LDR output for FXAA input (Bgra8UnormSrgb).
pub enum FxaaInput {}
impl_resource_tag!(FxaaInput, "fxaa_input");

// ---------------------------------------------------------------------------
// Shadow tags
// ---------------------------------------------------------------------------

/// Directional shadow depth map (Depth32Float).
pub enum ShadowDepth {}
impl_resource_tag!(ShadowDepth, "shadow_depth");

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::pass::ResHandle;

    /// Compile-time safety: different tags produce incompatible ResHandle types.
    #[test]
    fn compile_time_type_safety() {
        let pos: ResHandle<GPosition> = ResHandle::new(0);
        let norm: ResHandle<GNormal> = ResHandle::new(0);
        let albedo: ResHandle<GAlbedo> = ResHandle::new(0);
        let material: ResHandle<GMaterial> = ResHandle::new(0);
        let depth: ResHandle<GDepth> = ResHandle::new(0);
        let swap: ResHandle<Swapchain> = ResHandle::new(0);
        let ao: ResHandle<AOTexture> = ResHandle::new(0);
        let _irradiance: ResHandle<IrradianceMap> = ResHandle::new(0);
        let _prefiltered: ResHandle<PrefilteredMap> = ResHandle::new(0);
        let _brdf_lut: ResHandle<BrdfLUT> = ResHandle::new(0);
        let _reflection: ResHandle<ReflectionTexture> = ResHandle::new(0);

        // All have the same index — that's fine, types differ.
        // The compiler enforces: pos cannot be passed where norm is expected.
        assert_eq!(pos.index, 0);
        assert_eq!(norm.index, 0);
        assert_eq!(albedo.index, 0);
        assert_eq!(material.index, 0);
        assert_eq!(depth.index, 0);
        assert_eq!(swap.index, 0);
        assert_eq!(ao.index, 0);
    }

    /// Each resource tag exposes its logical graph name as a constant.
    #[test]
    fn resource_tag_names() {
        assert_eq!(GPosition::NAME, "gbuffer_position");
        assert_eq!(GNormal::NAME, "gbuffer_normal");
        assert_eq!(GAlbedo::NAME, "gbuffer_albedo");
        assert_eq!(GMaterial::NAME, "gbuffer_material");
        assert_eq!(GDepth::NAME, "gbuffer_depth");
        assert_eq!(Swapchain::NAME, "swapchain");
        assert_eq!(AOTexture::NAME, "ao");
        assert_eq!(AOTextureBlurred::NAME, "ao_blurred");
        assert_eq!(ShadowDepth::NAME, "shadow_depth");
    }

    /// No two tags can share the same logical graph name.
    #[test]
    fn resource_tag_names_are_unique() {
        let names = [
            GPosition::NAME,
            GNormal::NAME,
            GAlbedo::NAME,
            GMaterial::NAME,
            GDepth::NAME,
            Swapchain::NAME,
            AOTexture::NAME,
            AOTextureBlurred::NAME,
            SsrTraceResult::NAME,
            IrradianceMap::NAME,
            PrefilteredMap::NAME,
            BrdfLUT::NAME,
            ReflectionTexture::NAME,
            SceneColor::NAME,
            WaterColor::NAME,
            CloudColor::NAME,
            GodRayColor::NAME,
            PostProcessInput::NAME,
            BrightTexture::NAME,
            BloomMip0::NAME,
            BloomMip1::NAME,
            BloomMip2::NAME,
            BloomTexture::NAME,
            BloomResult::NAME,
            FxaaInput::NAME,
            ShadowDepth::NAME,
        ];
        let mut set = std::collections::HashSet::new();
        for name in names {
            assert!(set.insert(name), "duplicate resource tag name: {}", name);
        }
    }
}
