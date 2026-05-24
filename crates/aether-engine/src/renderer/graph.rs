use super::context::RenderContext;
use crate::ecs::World;
use std::collections::HashMap;
use tracing::trace;

/// Render graph.
///
/// Manages render pass execution order and transient resources.
/// Passes declare their inputs/outputs, and the graph handles
/// automatic dependency resolution and resource allocation.
pub struct RenderGraph {
    passes: Vec<Box<dyn RenderPass>>,
    resources: HashMap<String, GraphResource>,
}

impl Default for RenderGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderGraph {
    /// Create a new empty render graph.
    pub fn new() -> Self {
        Self {
            passes: Vec::new(),
            resources: HashMap::new(),
        }
    }

    /// Add a render pass to the graph.
    pub fn add_pass(&mut self, pass: impl RenderPass + 'static) {
        trace!("Adding render pass: {}", pass.name());
        self.passes.push(Box::new(pass));
    }

    /// Declare a graph resource.
    pub fn declare_resource(&mut self, name: impl Into<String>, desc: ResourceDesc) {
        let name = name.into();
        trace!("Declaring resource: {} ({:?})", name, desc);
        self.resources.insert(name, GraphResource::Declared(desc));
    }

    /// Execute the render graph.
    pub fn execute(
        &self,
        _encoder: &mut wgpu::CommandEncoder,
        _output_view: &wgpu::TextureView,
        _context: &RenderContext,
        _world: &World,
    ) {
        // TODO: Implement topological sort and pass execution
        // For now, passes are executed in registration order
        for pass in &self.passes {
            trace!("Executing pass: {}", pass.name());
            // pass.execute(encoder, context, world);
        }
    }
}

/// A render pass.
///
/// Each pass represents a distinct rendering stage (e.g., G-Buffer, Lighting, Post-Process).
pub trait RenderPass {
    /// Pass name for debugging.
    fn name(&self) -> &str;

    /// Declare resource requirements.
    fn declare_resources(&self, _builder: &mut PassResourceBuilder) {}

    /// Execute the pass.
    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        context: &RenderContext,
        world: &World,
    );
}

/// Resource description for render graph.
#[derive(Debug, Clone)]
pub enum ResourceDesc {
    /// Color texture.
    ColorTexture {
        /// Texture format.
        format: wgpu::TextureFormat,
        /// Width in pixels.
        width: u32,
        /// Height in pixels.
        height: u32,
    },
    /// Depth texture.
    DepthTexture {
        /// Texture format.
        format: wgpu::TextureFormat,
        /// Width in pixels.
        width: u32,
        /// Height in pixels.
        height: u32,
    },
    /// Buffer resource.
    Buffer {
        /// Buffer size in bytes.
        size: wgpu::BufferAddress,
        /// Buffer usage flags.
        usage: wgpu::BufferUsages,
    },
}

/// Builder for pass resource declarations.
pub struct PassResourceBuilder {
    reads: Vec<String>,
    writes: Vec<(String, ResourceDesc)>,
}

impl PassResourceBuilder {
    /// Read from a named resource.
    pub fn read(&mut self, name: impl Into<String>) {
        self.reads.push(name.into());
    }

    /// Write to a named resource.
    pub fn write(&mut self, name: impl Into<String>, desc: ResourceDesc) {
        self.writes.push((name.into(), desc));
    }
}

/// Internal resource state.
#[allow(dead_code)]
enum GraphResource {
    /// Resource has been declared but not yet allocated.
    Declared(ResourceDesc),
    /// Resource has been allocated on the GPU.
    Allocated(wgpu::Texture),
}
