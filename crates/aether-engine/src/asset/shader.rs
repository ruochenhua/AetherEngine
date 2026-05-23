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

    /// Register a shader.
    pub fn register(&mut self, name: impl Into<String>, source: impl Into<String>) {
        self.shaders.insert(name.into(), source.into());
    }

    /// Compile a shader module from source.
    pub fn compile(
        &self,
        device: &wgpu::Device,
        name: &str,
    ) -> Option<wgpu::ShaderModule> {
        let source = self.shaders.get(name)?;
        Some(device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(name),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        }))
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
    }
}
