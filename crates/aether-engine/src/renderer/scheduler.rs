//! Scheduler: executes render passes in topological order.
//!
//! Built by `PipelineBuilder::build()`, the Scheduler holds the ordered pass
//! list and the transient resource table. It dispatches frame data and executes
//! passes each frame.

use crate::renderer::frame::RenderFrame;
use crate::renderer::gpu_timer::GpuTimer;
use crate::renderer::pass::Pass;
#[cfg(test)]
use crate::renderer::pipeline_builder::compute_topological_order;
use crate::renderer::pipeline_builder::PipelineBuilder;
use crate::renderer::resource_table::ResourceTable;
use std::collections::HashMap;

/// Executes render passes in topological order.
pub struct Scheduler {
    /// Passes in execution order (after resolve).
    pub(crate) passes: Vec<Box<dyn Pass>>,
    /// Shared transient resource table.
    pub resource_table: ResourceTable,
}

impl Scheduler {
    /// Dispatch per-frame data to all passes via `apply_frame`.
    ///
    /// Called by the Launcher each frame before `execute_all`.
    pub fn apply_frame_all(&mut self, frame: &RenderFrame) {
        for pass in &mut self.passes {
            pass.apply_frame(frame);
        }
    }

    /// Execute all passes in topological order.
    ///
    /// Passes whose `should_run(frame)` returns `false` are skipped.
    /// If `timer` is provided, timestamp queries are written around the frame
    /// and each pass.
    pub fn execute_all(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        surface_view: &wgpu::TextureView,
        frame: &RenderFrame,
        timer: Option<&mut GpuTimer>,
    ) {
        if let Some(timer) = timer {
            encoder.write_timestamp(timer.query_set(), timer.frame_start_index());

            for (i, pass) in self.passes.iter().enumerate() {
                if pass.should_run(frame) {
                    encoder.write_timestamp(timer.query_set(), timer.pass_start_index(i));
                    pass.execute(encoder, &self.resource_table, surface_view);
                    encoder.write_timestamp(timer.query_set(), timer.pass_end_index(i));
                }
            }

            encoder.write_timestamp(timer.query_set(), timer.frame_end_index());
            timer.resolve(encoder);
        } else {
            for pass in &self.passes {
                if pass.should_run(frame) {
                    pass.execute(encoder, &self.resource_table, surface_view);
                }
            }
        }
    }

    /// Number of passes in the schedule.
    pub fn pass_count(&self) -> usize {
        self.passes.len()
    }

    /// Return the names of all scheduled passes in execution order.
    pub fn pass_names(&self) -> Vec<String> {
        self.passes.iter().map(|p| p.name().to_string()).collect()
    }

    /// Find the first pass of type `T` and return a mutable reference.
    fn pass_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.passes
            .iter_mut()
            .find_map(|p| p.as_any_mut().downcast_mut::<T>())
    }

    /// Set screen size on the SSAOPass (for texel-accurate blur).
    pub fn set_ssao_screen_size(&mut self, width: u32, height: u32) {
        if let Some(ssao) = self.pass_mut::<crate::renderer::passes::ssao::SSAOPass>() {
            ssao.set_screen_size(width, height);
        }
    }

    /// Set dynamic debug lines on the DebugLinePass.
    pub fn set_dynamic_lines(&mut self, lines: Vec<crate::renderer::passes::debug::DebugVertex>) {
        if let Some(debug) = self.pass_mut::<crate::renderer::passes::debug::DebugLinePass>() {
            debug.set_dynamic_lines(lines);
        }
    }

    /// Set screen size on the AOBlurPass.
    pub fn set_ao_blur_screen_size(&mut self, width: u32, height: u32) {
        if let Some(blur) = self.pass_mut::<crate::renderer::passes::ao_blur::AOBlurPass>() {
            blur.set_screen_size(width, height);
        }
    }

    /// Set screen size on the SSRPass (for textureLoad pixel coords).
    pub fn set_ssr_screen_size(&mut self, width: u32, height: u32) {
        if let Some(ssr) = self.pass_mut::<crate::renderer::passes::ssr::SSRPass>() {
            ssr.set_screen_size(width, height);
        }
    }

    /// Set the debug visualization mode on the LightingPass.
    pub fn set_debug_mode(&mut self, mode: u32) {
        if let Some(lp) = self.pass_mut::<crate::renderer::passes::lighting::LightingPass>() {
            lp.set_debug_mode(mode);
        }
    }

    /// Set the SSR debug visualization mode.
    pub fn set_ssr_debug_mode(&mut self, mode: u32) {
        if let Some(ssr) = self.pass_mut::<crate::renderer::passes::ssr::SSRPass>() {
            ssr.set_debug_mode(mode);
        }
    }

    /// Enable or disable SSR.
    pub fn set_ssr_enabled(&mut self, enabled: bool) {
        if let Some(ssr) = self.pass_mut::<crate::renderer::passes::ssr::SSRPass>() {
            ssr.set_enabled(enabled);
        }
    }

    /// Set SSAO parameters (radius, bias, intensity).
    pub fn set_ssao_params(&mut self, radius: f32, bias: f32, intensity: f32) {
        if let Some(ssao) = self.pass_mut::<crate::renderer::passes::ssao::SSAOPass>() {
            ssao.set_radius(radius);
            ssao.set_bias(bias);
            ssao.set_intensity(intensity);
        }
    }

    /// Toggle rendering features in the LightingPass.
    pub fn set_feature_flags(&mut self, ssao: bool, shadow: bool, ibl: bool) {
        if let Some(lp) = self.pass_mut::<crate::renderer::passes::lighting::LightingPass>() {
            lp.set_ssao_enabled(ssao);
            lp.set_shadow_enabled(shadow);
            lp.set_ibl_enabled(ibl);
        }
    }

    /// Set tone mapping mode on the ToneMappingPass.
    pub fn set_tone_mapping_mode(
        &mut self,
        mode: crate::renderer::passes::tone_mapping::ToneMappingMode,
        queue: &wgpu::Queue,
    ) {
        if let Some(tmp) = self.pass_mut::<crate::renderer::passes::tone_mapping::ToneMappingPass>()
        {
            tmp.set_mode(mode);
            tmp.update_uniforms(queue);
        }
    }

    /// Set bloom parameters.
    pub fn set_bloom_params(
        &mut self,
        enabled: bool,
        threshold: f32,
        intensity: f32,
        bloom_intensity: f32,
        queue: &wgpu::Queue,
    ) {
        if let Some(bloom) = self.pass_mut::<crate::renderer::passes::bloom::BloomPass>() {
            bloom.set_enabled(enabled);
            bloom.set_threshold(threshold);
            bloom.set_intensity(intensity);
            bloom.set_bloom_intensity(bloom_intensity);
            bloom.update_uniforms(queue);
        }
    }

    /// Set bloom screen size (call before rebuild).
    pub fn set_bloom_screen_size(&mut self, width: u32, height: u32) {
        if let Some(bloom) = self.pass_mut::<crate::renderer::passes::bloom::BloomPass>() {
            bloom.set_screen_size(width, height);
        }
    }

    /// Set FXAA parameters.
    pub fn set_fxaa_params(
        &mut self,
        enabled: bool,
        quality: crate::renderer::passes::fxaa::FxaaQuality,
        edge_threshold: Option<f32>,
        queue: &wgpu::Queue,
    ) {
        if let Some(fxaa) = self.pass_mut::<crate::renderer::passes::fxaa::FXAAPass>() {
            fxaa.set_enabled(enabled);
            fxaa.set_quality(quality);
            fxaa.set_edge_threshold(edge_threshold);
            fxaa.update_uniforms_with_queue(queue);
        }
    }

    /// Rebuild resolution-dependent resources after a resize.
    pub fn rebuild(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        // Re-allocate all textures and re-resolve all passes
        let mut new_table = ResourceTable::new();

        // Collect signature writes from passes
        let mut seen: HashMap<(std::any::TypeId, &str), bool> = HashMap::new();
        for pass in &self.passes {
            let sig = pass.signature();
            for write_slot in &sig.writes {
                let key = (write_slot.type_id, write_slot.name);
                if seen.contains_key(&key) {
                    continue;
                }
                seen.insert(key, true);

                let format = write_slot.format.expect("Write slot must have format");
                let tex_width = write_slot.width.unwrap_or(width);
                let tex_height = write_slot.height.unwrap_or(height);
                let layers = write_slot.layers.unwrap_or(1);
                let texture = PipelineBuilder::create_transient_texture(
                    device, format, tex_width, tex_height, layers,
                );
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                new_table.allocate_with_texture(write_slot.type_id, write_slot.name, texture, view);
            }
        }

        self.resource_table = new_table;

        for pass in &mut self.passes {
            pass.resolve(device, &self.resource_table);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::pass::{PassSignature, ResSlot, SlotKind};
    use crate::renderer::passes::debug::DebugLinePass;
    use crate::renderer::passes::gbuffer::GBufferPass;
    use crate::renderer::passes::lighting::LightingPass;
    use crate::renderer::passes::shadow::ShadowPass;
    use crate::renderer::resource::*;
    use std::any::TypeId;
    use std::sync::Mutex;

    struct MockPass {
        name: &'static str,
        reads: Vec<ResSlot>,
        writes: Vec<ResSlot>,
        order_log: std::sync::Arc<Mutex<Vec<String>>>,
    }

    impl MockPass {
        fn new(name: &'static str, order_log: std::sync::Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                name,
                reads: Vec::new(),
                writes: Vec::new(),
                order_log,
            }
        }

        fn with_write<T: ResourceTag>(
            mut self,
            name: &'static str,
            format: wgpu::TextureFormat,
        ) -> Self {
            self.writes.push(ResSlot {
                type_id: TypeId::of::<T>(),
                name,
                format: Some(format),
                kind: SlotKind::Write,
                width: None,
                height: None,
                layers: None,
            });
            self
        }

        fn with_read<T: ResourceTag>(mut self, name: &'static str) -> Self {
            self.reads.push(ResSlot {
                type_id: TypeId::of::<T>(),
                name,
                format: None,
                kind: SlotKind::Read,
                width: None,
                height: None,
                layers: None,
            });
            self
        }
    }

    impl Pass for MockPass {
        fn name(&self) -> &str {
            self.name
        }
        fn signature(&self) -> PassSignature {
            PassSignature {
                name: self.name,
                reads: self.reads.clone(),
                writes: self.writes.clone(),
            }
        }
        fn init(_device: &wgpu::Device) -> Self {
            panic!("MockPass does not support init()")
        }
        fn execute(
            &self,
            _encoder: &mut wgpu::CommandEncoder,
            _resources: &ResourceTable,
            _surface_view: &wgpu::TextureView,
        ) {
            self.order_log.lock().unwrap().push(self.name.to_string());
        }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    fn build_mock_scheduler(passes: Vec<MockPass>) -> Scheduler {
        let boxed: Vec<Box<dyn Pass>> = passes
            .into_iter()
            .map(|p| Box::new(p) as Box<dyn Pass>)
            .collect();
        let _order = compute_topological_order(&boxed);

        Scheduler {
            passes: boxed,
            resource_table: ResourceTable::new(),
        }
    }

    #[test]
    fn single_pass_executes() {
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        let pass = MockPass::new("Single", log.clone());
        let s = build_mock_scheduler(vec![pass]);
        assert_eq!(s.passes.len(), 1);
    }

    #[test]
    fn dependency_order_before_dependent() {
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        let a = MockPass::new("Producer", log.clone())
            .with_write::<GPosition>("pos", wgpu::TextureFormat::Rgba16Float);
        let b = MockPass::new("Consumer", log.clone()).with_read::<GPosition>("pos");
        let passes = vec![a, b];
        let boxed: Vec<Box<dyn Pass>> = passes
            .into_iter()
            .map(|p| Box::new(p) as Box<dyn Pass>)
            .collect();
        let order = compute_topological_order(&boxed);
        assert_eq!(boxed[order[0]].name(), "Producer");
        assert_eq!(boxed[order[1]].name(), "Consumer");
    }

    #[test]
    fn three_pass_chain() {
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        let a = MockPass::new("A", log.clone())
            .with_write::<GPosition>("pos", wgpu::TextureFormat::Rgba16Float);
        let b = MockPass::new("B", log.clone())
            .with_read::<GPosition>("pos")
            .with_write::<GNormal>("norm", wgpu::TextureFormat::Rgba16Float);
        let c = MockPass::new("C", log.clone()).with_read::<GNormal>("norm");
        let passes = vec![a, b, c];
        let boxed: Vec<Box<dyn Pass>> = passes
            .into_iter()
            .map(|p| Box::new(p) as Box<dyn Pass>)
            .collect();
        let order = compute_topological_order(&boxed);
        assert_eq!(boxed[order[0]].name(), "A");
        assert_eq!(boxed[order[1]].name(), "B");
        assert_eq!(boxed[order[2]].name(), "C");
    }

    #[test]
    fn independent_passes_keep_registration_order() {
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        let a = MockPass::new("First", log.clone())
            .with_write::<GPosition>("pos", wgpu::TextureFormat::Rgba16Float);
        let b = MockPass::new("Second", log.clone())
            .with_write::<GNormal>("norm", wgpu::TextureFormat::Rgba16Float);
        let passes = vec![a, b];
        let boxed: Vec<Box<dyn Pass>> = passes
            .into_iter()
            .map(|p| Box::new(p) as Box<dyn Pass>)
            .collect();
        let order = compute_topological_order(&boxed);
        assert_eq!(boxed[order[0]].name(), "First");
        assert_eq!(boxed[order[1]].name(), "Second");
    }

    #[test]
    #[should_panic(expected = "Missing producer")]
    fn missing_producer_panics() {
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        let pass = MockPass::new("Consumer", log.clone()).with_read::<GPosition>("nonexistent");
        let boxed: Vec<Box<dyn Pass>> = vec![Box::new(pass)];
        compute_topological_order(&boxed);
    }

    #[test]
    fn sequential_writers_are_ordered() {
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        let a = MockPass::new("A", log.clone())
            .with_write::<GPosition>("same", wgpu::TextureFormat::Rgba16Float);
        let b = MockPass::new("B", log.clone())
            .with_write::<GPosition>("same", wgpu::TextureFormat::Rgba16Float);
        let boxed: Vec<Box<dyn Pass>> = vec![Box::new(a), Box::new(b)];
        let order = compute_topological_order(&boxed);
        assert_eq!(boxed[order[0]].name(), "A");
        assert_eq!(boxed[order[1]].name(), "B");
    }

    #[test]
    #[should_panic(expected = "Dependency cycle")]
    fn cycle_detection() {
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        let a = MockPass::new("A", log.clone())
            .with_write::<GPosition>("x", wgpu::TextureFormat::Rgba16Float)
            .with_read::<GNormal>("y");
        let b = MockPass::new("B", log.clone())
            .with_write::<GNormal>("y", wgpu::TextureFormat::Rgba16Float)
            .with_read::<GPosition>("x");
        let boxed: Vec<Box<dyn Pass>> = vec![Box::new(a), Box::new(b)];
        compute_topological_order(&boxed);
    }

    #[test]
    fn pass_with_no_dependencies_works() {
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        let pass = MockPass::new("Orphan", log.clone())
            .with_write::<Swapchain>("output", wgpu::TextureFormat::Bgra8Unorm);
        let boxed: Vec<Box<dyn Pass>> = vec![Box::new(pass)];
        let order = compute_topological_order(&boxed);
        assert_eq!(order, vec![0]);
    }

    #[test]
    fn rebuild_reallocates_textures() {
        let device = headless_device();
        let pass_a = ShadowPass::new(&device);
        let pass_b = GBufferPass::new(&device);
        let pass_ssao = crate::renderer::passes::ssao::SSAOPass::new(&device);
        let pass_blur = crate::renderer::passes::ao_blur::AOBlurPass::new(&device);
        let pass_c = LightingPass::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb);

        let passes: Vec<&dyn Pass> = vec![
            &pass_a as &dyn Pass,
            &pass_b as &dyn Pass,
            &pass_ssao as &dyn Pass,
            &pass_blur as &dyn Pass,
            &pass_c as &dyn Pass,
        ];
        let table = PipelineBuilder::validate_and_allocate(&passes, &device, 64, 64);
        assert!(table.len() >= 7); // shadow + 4 GBuffer + depth + AO + AO_blurred

        let table2 = PipelineBuilder::validate_and_allocate(&passes, &device, 128, 128);
        assert!(table2.len() >= 7);
    }

    #[test]
    fn build_all_passes_works() {
        let device = headless_device();
        let sf = wgpu::TextureFormat::Bgra8UnormSrgb;
        let df = wgpu::TextureFormat::Depth32Float;
        let debug_pass = DebugLinePass::new(&device, sf, df);

        let scheduler = PipelineBuilder::new()
            .add_pass(ShadowPass::new(&device))
            .add_pass(GBufferPass::new(&device))
            .add_pass(crate::renderer::passes::ssao::SSAOPass::new(&device))
            .add_pass(crate::renderer::passes::ao_blur::AOBlurPass::new(&device))
            .add_pass(LightingPass::new(&device, sf))
            .add_pass(debug_pass)
            .build(&device, 64, 64);

        assert_eq!(scheduler.pass_count(), 6);
    }

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
}
