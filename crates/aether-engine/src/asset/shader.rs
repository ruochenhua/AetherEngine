use std::collections::HashMap;

/// Shader module manager.
///
/// Manages WGSL shader source code and compilation.
pub struct ShaderLibrary {
    shaders: HashMap<String, String>,
}

impl Default for ShaderLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl ShaderLibrary {
    /// Create a new shader library.
    pub fn new() -> Self {
        let mut lib = Self {
            shaders: HashMap::new(),
        };
        lib.register_builtin_shaders();
        lib
    }

    /// Get a shader by name.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.shaders.get(name).map(|s| s.as_str())
    }

    /// Validate and create a shader module.
    ///
    /// Returns an error with the WGSL parse message instead of letting wgpu
    /// surface a less actionable runtime validation failure later.
    pub fn create_shader_module(
        device: &wgpu::Device,
        label: &str,
        source: &str,
    ) -> anyhow::Result<wgpu::ShaderModule> {
        naga::front::wgsl::parse_str(source)
            .map_err(|e| anyhow::anyhow!("WGSL parse error in {label}: {e}"))?;
        Ok(device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        }))
    }

    /// Register a shader.
    pub fn register(&mut self, name: impl Into<String>, source: impl Into<String>) {
        self.shaders.insert(name.into(), source.into());
    }

    /// Compile a shader module from source.
    pub fn compile(&self, device: &wgpu::Device, name: &str) -> Option<wgpu::ShaderModule> {
        let source = self.shaders.get(name)?;
        Self::create_shader_module(device, name, source).ok()
    }

    fn register_builtin_shaders(&mut self) {
        // Basic fullscreen quad vertex shader
        self.register(
            "fullscreen_quad.vert",
            include_str!("../../../../assets/shaders/fullscreen_quad.vert.wgsl"),
        );

        // Basic fragment shaders
        self.register(
            "solid_color.frag",
            include_str!("../../../../assets/shaders/solid_color.frag.wgsl"),
        );

        // Terrain splatting fragment shader (compile-only foundation for Phase 5)
        self.register(
            "terrain_splat.frag",
            include_str!("../../../../assets/shaders/terrain_splat.frag.wgsl"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headless_device() -> wgpu::Device {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("need adapter");
        let (device, _queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("need device");
        device
    }

    #[test]
    fn terrain_splat_shader_compiles() {
        let device = headless_device();
        let lib = ShaderLibrary::new();
        let module = lib.compile(&device, "terrain_splat.frag");
        assert!(module.is_some(), "terrain splat shader should compile");
    }

    #[test]
    fn fullscreen_quad_shader_compiles() {
        let device = headless_device();
        let lib = ShaderLibrary::new();
        let module = lib.compile(&device, "fullscreen_quad.vert");
        assert!(module.is_some(), "fullscreen quad shader should compile");
    }
    #[test]
    fn invalid_shader_returns_parse_error() {
        let device = headless_device();
        let result = ShaderLibrary::create_shader_module(&device, "bad", "@invalid wgsl");
        assert!(result.is_err());
    }
}
