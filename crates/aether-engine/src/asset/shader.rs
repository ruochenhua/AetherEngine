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
    pub fn compile(&self, device: &wgpu::Device, name: &str) -> Option<wgpu::ShaderModule> {
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
    use crate::test_utils::headless_device_queue;

    #[test]
    fn terrain_splat_shader_compiles() {
        let Some((device, _queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let lib = ShaderLibrary::new();
        let module = lib.compile(&device, "terrain_splat.frag");
        assert!(module.is_some(), "terrain splat shader should compile");
    }

    #[test]
    fn fullscreen_quad_shader_compiles() {
        let Some((device, _queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let lib = ShaderLibrary::new();
        let module = lib.compile(&device, "fullscreen_quad.vert");
        assert!(module.is_some(), "fullscreen quad shader should compile");
    }
}
