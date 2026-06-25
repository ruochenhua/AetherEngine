//! Configuration for IBL precomputation.

/// Configuration for IBL precomputation.
pub struct IblConfig {
    /// Environment cubemap size per face (default: 512).
    pub env_size: u32,
    /// Irradiance cubemap size per face (default: 32).
    pub irradiance_size: u32,
    /// Prefiltered cubemap base size per face (default: 128).
    pub prefilter_size: u32,
    /// Number of mip levels for prefiltered cubemap (default: 5).
    pub prefilter_mips: u32,
    /// BRDF LUT size (default: 256).
    pub brdf_lut_size: u32,
    /// Path to HDR environment map.
    pub environment_path: Option<String>,
    /// When true, use a magenta/cyan checkerboard instead of HDR file (for debugging).
    pub debug_checkerboard: bool,
}

impl Default for IblConfig {
    fn default() -> Self {
        Self {
            env_size: 512,
            irradiance_size: 32,
            prefilter_size: 128,
            prefilter_mips: 5,
            brdf_lut_size: 256,
            environment_path: None,
            debug_checkerboard: false,
        }
    }
}
