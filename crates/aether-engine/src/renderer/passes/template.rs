//! Pass template — copy this file to create a new render pass.
//!
//! ## Steps to add a new pass
//!
//! 1. Copy this file to `passes/your_pass.rs`
//! 2. Replace `TemplatePass` with your pass name
//! 3. Fill in `signature()` — declare reads/writes with type tags
//! 4. Fill in `init()` — create pipelines, shaders, uniform buffers
//! 5. Fill in `resolve()` — obtain `ResHandle<T>` handles, create bind groups
//! 6. Fill in `execute()` — record render commands
//! 7. Register in `passes/mod.rs`: `pub mod your_pass;`
//! 8. Register in `main.rs`: add to the pass list + set per-frame data
//!
//! ## Anatomy
//!
//! - **Type tags** (`GPosition`, `AOTexture`, etc.) are zero-size markers
//!   defined in `renderer::resource`. Add new ones if your pass produces a
//!   new resource type.
//! - **`init()`** runs once. No texture access — just pipelines, shaders,
//!   uniform buffers.
//! - **`resolve()`** runs after transient textures are allocated. Create
//!   bind groups that reference textures here.
//! - **`execute()`** runs every frame. Record GPU commands. Do NOT create
//!   new GPU resources here — pre-create in init/resolve.
//! - **Per-frame data** goes through pass-specific setters (e.g.
//!   `set_frame_data`). The Launcher calls these before `execute()`.
//! - **Inline WGSL** as `Cow::Borrowed` raw strings — keeps the pass
//!   self-contained for AI agents.

use crate::renderer::pass::{Pass, PassSignature};
use crate::renderer::resource_table::ResourceTable;

/// TODO: Rename to your pass name.
pub struct TemplatePass {
    // TODO: Add your GPU resources here.
    // Examples:
    // - wgpu::RenderPipeline
    // - wgpu::Buffer (uniform buffers)
    // - wgpu::BindGroup
    // - Option<ResHandle<YourTag>> (populated in resolve)
    #[allow(dead_code)]
    placeholder: u32,

    // TODO: Add per-frame state here.
    // Examples:
    // - view/proj matrices
    // - renderable lists
    // - debug flags
}

impl Pass for TemplatePass {
    fn name(&self) -> &str {
        "Template" // TODO: Change to your pass name
    }

    fn signature(&self) -> PassSignature {
        // TODO: Declare reads and writes.
        //
        // Reads: resources produced by earlier passes that this pass consumes.
        //   .read::<TypeTag>("resource_name")
        //
        // Writes: resources this pass produces for later passes.
        //   .write::<TypeTag>("resource_name", format)
        //
        // Example:
        //   PassSignature::new("SSAO")
        //       .read::<GPosition>("gbuffer_position")
        //       .read::<GNormal>("gbuffer_normal")
        //       .write::<AOTexture>("ao", wgpu::TextureFormat::R8Unorm)

        PassSignature::new("Template")
    }

    fn init(device: &wgpu::Device) -> Self
    where
        Self: Sized,
    {
        // TODO: Create pipelines, shaders, uniform buffers.
        //
        // NO texture access here — textures are created by the Scheduler
        // after all signatures are collected.
        //
        // Example shader creation:
        //   let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        //       label: Some("Template Shader"),
        //       source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(
        //           r#" ... WGSL here ... "#,
        //       )),
        //   });

        // TODO: Replace this placeholder with real initialization.
        let _ = device; // Silence unused warning — remove when real code is added.

        Self {
            placeholder: 0, // TODO: Replace with your fields.
        }
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        // TODO: Obtain handles and create texture-dependent bind groups.
        //
        // Use `resources.handle::<TypeTag>("name")` to get ResHandle<T>.
        // Then create bind groups referencing the texture views.
        //
        // Example:
        //   self.pos_handle = Some(resources.handle::<GPosition>("gbuffer_position"));
        //   let view = resources.get(self.pos_handle.unwrap());
        //   let sampler = device.create_sampler(&Default::default());
        //   let bg = device.create_bind_group(&wgpu::BindGroupDescriptor { ... });

        let _ = device;
        let _ = resources;
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        surface_view: &wgpu::TextureView,
    ) {
        // TODO: Record render commands.
        //
        // - Begin a render pass with `encoder.begin_render_pass(...)`
        // - Upload uniforms with `queue.write_buffer(...)`
        // - Set pipeline, bind groups, vertex buffers
        // - Draw with `pass.draw(...)` or `pass.draw_indexed(...)`
        //
        // Example:
        //   let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        //       label: Some("Template"),
        //       color_attachments: &[Some(wgpu::RenderPassColorAttachment {
        //           view: surface_view,
        //           resolve_target: None,
        //           ops: wgpu::Operations {
        //               load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        //               store: wgpu::StoreOp::Store,
        //           },
        //       })],
        //       depth_stencil_attachment: None,
        //       timestamp_writes: None,
        //       occlusion_query_set: None,
        //   });
        //   pass.set_pipeline(&self.pipeline);

        let _ = encoder;
        let _ = resources;
        let _ = surface_view;
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl TemplatePass {
    // TODO: Add pass-specific methods.
    //
    // Per-frame data setters (called by the Launcher before execute):
    //   pub fn set_frame_data(&mut self, ...) { ... }
    //
    // Uniform update helpers:
    //   pub fn update_uniforms(&self, queue: &wgpu::Queue, data: &Uniforms) {
    //       queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[*data]));
    //   }
}

// TODO: Add unit tests.
//
// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     #[test]
//     fn signature_has_expected_reads_and_writes() {
//         // Verify signature is correct
//     }
//
//     #[test]
//     fn init_does_not_panic() {
//         // Verify init creates resources without errors
//     }
// }
