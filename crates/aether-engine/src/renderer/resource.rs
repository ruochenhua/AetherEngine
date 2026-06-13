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
pub trait ResourceTag: 'static {}

// ---------------------------------------------------------------------------
// G-Buffer tags
// ---------------------------------------------------------------------------

/// G-Buffer position texture (RGBA16Float).
pub enum GPosition {}
impl ResourceTag for GPosition {}

/// G-Buffer normal texture (RGBA16Float).
pub enum GNormal {}
impl ResourceTag for GNormal {}

/// G-Buffer albedo texture (RGBA8Unorm).
pub enum GAlbedo {}
impl ResourceTag for GAlbedo {}

/// G-Buffer material texture (RG8Unorm: roughness, metallic).
pub enum GMaterial {}
impl ResourceTag for GMaterial {}

/// G-Buffer depth texture (Depth32Float).
pub enum GDepth {}
impl ResourceTag for GDepth {}

// ---------------------------------------------------------------------------
// Pipeline tags
// ---------------------------------------------------------------------------

/// Swapchain output texture.
pub enum Swapchain {}
impl ResourceTag for Swapchain {}

// ---------------------------------------------------------------------------
// Screen-space effect tags
// ---------------------------------------------------------------------------

/// Ambient occlusion texture (R8Unorm).
pub enum AOTexture {}
impl ResourceTag for AOTexture {}

/// Blurred ambient occlusion texture (R8Unorm).
pub enum AOTextureBlurred {}
impl ResourceTag for AOTextureBlurred {}

/// Irradiance cubemap (Rgba16Float, 32×32 per face).
pub enum IrradianceMap {}
impl ResourceTag for IrradianceMap {}

/// Prefiltered specular cubemap (Rgba16Float, 128×128 base, 5 mip levels).
pub enum PrefilteredMap {}
impl ResourceTag for PrefilteredMap {}

/// BRDF integration LUT (Rgba16Float 2D, 256×256).
pub enum BrdfLUT {}
impl ResourceTag for BrdfLUT {}

/// Screen-space reflection texture (R11G11B10Float, half-resolution).
pub enum ReflectionTexture {}
impl ResourceTag for ReflectionTexture {}

/// Scene color texture after deferred lighting (Rgba16Float).
pub enum SceneColor {}
impl ResourceTag for SceneColor {}

/// Water overlay color/alpha (Rgba16Float).
pub enum WaterColor {}
impl ResourceTag for WaterColor {}

/// Volumetric cloud overlay color/alpha (Rgba16Float).
pub enum CloudColor {}
impl ResourceTag for CloudColor {}

/// God ray light-shaft overlay (Rgba16Float).
pub enum GodRayColor {}
impl ResourceTag for GodRayColor {}

/// Post-process input texture — HDR linear output from composite (Rgba16Float).
pub enum PostProcessInput {}
impl ResourceTag for PostProcessInput {}

/// Bright regions extracted for bloom (Rgba16Float).
pub enum BrightTexture {}
impl ResourceTag for BrightTexture {}

/// Bloom mip level 0 — half resolution (Rgba16Float).
pub enum BloomMip0 {}
impl ResourceTag for BloomMip0 {}

/// Bloom mip level 1 — quarter resolution (Rgba16Float).
pub enum BloomMip1 {}
impl ResourceTag for BloomMip1 {}

/// Bloom mip level 2 — eighth resolution (Rgba16Float).
pub enum BloomMip2 {}
impl ResourceTag for BloomMip2 {}

/// Final blurred bloom texture — full resolution (Rgba16Float).
pub enum BloomTexture {}
impl ResourceTag for BloomTexture {}

/// Bloom composited with HDR scene (Rgba16Float).
pub enum BloomResult {}
impl ResourceTag for BloomResult {}

/// Tone-mapped LDR output for FXAA input (Bgra8UnormSrgb).
pub enum FxaaInput {}
impl ResourceTag for FxaaInput {}

// ---------------------------------------------------------------------------
// Shadow tags
// ---------------------------------------------------------------------------

/// Directional shadow depth map (Depth32Float).
pub enum ShadowDepth {}
impl ResourceTag for ShadowDepth {}

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
}
