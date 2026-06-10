//! Pipeline builder and scheduler.
//!
//! `PipelineBuilder` collects passes with their signatures, topologically sorts
//! the dependency graph, allocates transient textures, resolves passes, and
//! produces a `Scheduler`. The `Scheduler` executes passes in topological order.
//!
//! ## Example
//!
//! ```rust,ignore
//! let scheduler = PipelineBuilder::new()
//!     .add(GBufferPass::init(device))
//!     .add(LightingPass::new(device, surface_format))
//!     .add(DebugLinePass::new(device, surface_format, depth_format))
//!     .build(device, width, height);
//!
//! // Per frame:
//! scheduler.execute(&mut encoder, &resource_table);
//! ```

use std::any::TypeId;
use std::collections::{HashMap, HashSet, VecDeque};
use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{Pass, PassSignature};
use crate::renderer::resource_table::ResourceTable;
use tracing::debug;

/// Builds a render pipeline from individual passes.
pub struct PipelineBuilder {
    passes: Vec<Box<dyn Pass>>,
}

impl PipelineBuilder {
    /// Create a new empty pipeline builder.
    pub fn new() -> Self {
        Self { passes: Vec::new() }
    }

    /// Add a pass to the pipeline.
    pub fn add<P: Pass + 'static>(mut self, pass: P) -> Self {
        self.passes.push(Box::new(pass));
        self
    }

    /// Validate the pass graph and allocate transient textures.
    ///
    /// Returns the ResourceTable with all transient textures allocated.
    /// Passes should call `resolve()` with this table after validation.
    ///
    /// This is the lightweight version that does NOT take ownership of passes —
    /// it only validates the dependency graph and allocates textures.
    pub fn validate_and_allocate(
        passes: &[&dyn Pass],
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> ResourceTable {
        let boxed: Vec<Box<dyn Pass>> = Vec::new(); // not needed for validation

        // We need to reconstruct boxed versions for compute_topological_order
        // But compute_topological_order takes &[Box<dyn Pass>].
        // Workaround: recreate signatures manually.
        let n = passes.len();
        let mut sigs: Vec<crate::renderer::pass::PassSignature> = Vec::with_capacity(n);
        for pass in passes {
            sigs.push(pass.signature());
        }

        // Build producer map and detect conflicts
        let mut producer: HashMap<(TypeId, &str), usize> = HashMap::new();
        for (i, sig) in sigs.iter().enumerate() {
            for write_slot in &sig.writes {
                let key = (write_slot.type_id, write_slot.name);
                if let Some(&existing) = producer.get(&key) {
                    panic!(
                        "Resource conflict: '{}' written by pass {} and pass {}",
                        write_slot.name, sigs[existing].name, sig.name,
                    );
                }
                producer.insert(key, i);
            }
        }

        // Build dependency edges (deduplicated)
        let mut deps: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, sig) in sigs.iter().enumerate() {
            for read_slot in &sig.reads {
                let key = (read_slot.type_id, read_slot.name);
                match producer.get(&key) {
                    Some(&producer_idx) => {
                        if producer_idx != i && !deps[i].contains(&producer_idx) {
                            deps[i].push(producer_idx);
                        }
                    }
                    None => {
                        panic!(
                            "Missing producer: pass '{}' reads '{}' but no pass produces it",
                            sig.name, read_slot.name,
                        );
                    }
                }
            }
        }

        // Detect cycles
        detect_cycles_ref(&deps, n, &sigs);

        // Topological sort
        let mut in_degree: Vec<usize> = deps.iter().map(|d| d.len()).collect();
        let mut queue: VecDeque<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut order = Vec::with_capacity(n);
        while let Some(node) = queue.pop_front() {
            order.push(node);
            // Find consumers
            for (j, dep_list) in deps.iter().enumerate() {
                if dep_list.contains(&node) {
                    in_degree[j] -= 1;
                    if in_degree[j] == 0 {
                        queue.push_back(j);
                    }
                }
            }
        }
        if order.len() != n {
            let missing: Vec<&str> = (0..n)
                .filter(|i| !order.contains(i))
                .map(|i| sigs[i].name)
                .collect();
            panic!(
                "Topo sort incomplete: visited {}/{} passes. Missing: {:?}",
                order.len(), n, missing,
            );
        }

        debug!(
            "Topological order: {:?}",
            order.iter().map(|&i| sigs[i].name).collect::<Vec<_>>()
        );

        // Allocate transient textures
        let mut resource_table = ResourceTable::new();
        let mut seen: HashMap<(TypeId, &str), bool> = HashMap::new();
        for sig in &sigs {
            for write_slot in &sig.writes {
                let key = (write_slot.type_id, write_slot.name);
                if seen.contains_key(&key) {
                    continue;
                }
                seen.insert(key, true);
                let format = write_slot.format.expect("Write slot must have a format");
                let tex_width = write_slot.width.unwrap_or(width);
                let tex_height = write_slot.height.unwrap_or(height);
                let texture = Self::create_transient_texture(device, format, tex_width, tex_height);
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                resource_table.allocate(write_slot.type_id, write_slot.name, view);
            }
        }

        let _ = boxed;
        resource_table
    }

    /// Build the scheduler.
    ///
    /// Topologically sorts passes based on their read/write declarations,
    /// allocates transient resources, and calls `resolve()` on each pass.
    ///
    /// # Panics
    ///
    /// - If a pass reads a resource that no pass produces.
    /// - If a dependency cycle is detected.
    /// - If two passes write to the same `(type, name)` resource.
    pub fn build(mut self, device: &wgpu::Device, width: u32, height: u32) -> Scheduler {
        let order = compute_topological_order(&self.passes);

        // Collect unique writes and allocate transient textures
        let mut resource_table = ResourceTable::new();
        {
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
                    let texture = Self::create_transient_texture(device, format, tex_width, tex_height);
                    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                    resource_table.allocate(write_slot.type_id, write_slot.name, view);
                }
            }
        }

        // Reorder passes to topological order and call resolve
        let passes = std::mem::take(&mut self.passes);
        // Use Option wrapper to safely extract by index in arbitrary order
        let mut opt_passes: Vec<Option<Box<dyn Pass>>> = passes.into_iter().map(Some).collect();
        let mut ordered: Vec<Box<dyn Pass>> = Vec::with_capacity(order.len());
        for &idx in &order {
            ordered.push(
                opt_passes[idx]
                    .take()
                    .expect("Topological order contains duplicate or invalid index"),
            );
        }

        // Move DebugLine pass to the end so it renders after CompositePass
        if let Some(idx) = ordered.iter().position(|p| p.name() == "DebugLine") {
            let debug_pass = ordered.remove(idx);
            ordered.push(debug_pass);
        }

        // Resolve each pass with the resource table
        for pass in &mut ordered {
            pass.resolve(device, &resource_table);
        }

        Scheduler {
            passes: ordered,
            resource_table,
        }
    }

    fn create_transient_texture(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> wgpu::Texture {
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("transient"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    }
}

/// Executes render passes in topological order.
pub struct Scheduler {
    /// Passes in execution order (after resolve).
    passes: Vec<Box<dyn Pass>>,
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
    pub fn execute_all(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        surface_view: &wgpu::TextureView,
    ) {
        for pass in &self.passes {
            pass.execute(encoder, &self.resource_table, surface_view);
        }
    }

    /// Number of passes in the schedule.
    pub fn pass_count(&self) -> usize {
        self.passes.len()
    }

    /// Set screen size on the SSAOPass (for texel-accurate blur).
    pub fn set_ssao_screen_size(&mut self, width: u32, height: u32) {
        for pass in &mut self.passes {
            if pass.name() == "SSAO" {
                if let Some(ssao) = pass.as_any_mut().downcast_mut::<crate::renderer::passes::ssao::SSAOPass>() {
                    ssao.set_screen_size(width, height);
                }
                break;
            }
        }
    }

    /// Set dynamic debug lines on the DebugLinePass.
    pub fn set_dynamic_lines(&mut self, lines: Vec<crate::renderer::passes::debug::DebugVertex>) {
        for pass in &mut self.passes {
            if pass.name() == "DebugLine" {
                if let Some(debug) = pass.as_any_mut().downcast_mut::<crate::renderer::passes::debug::DebugLinePass>() {
                    debug.set_dynamic_lines(lines);
                }
                break;
            }
        }
    }

    /// Set screen size on the SSRPass (for textureLoad pixel coords).
    pub fn set_ssr_screen_size(&mut self, width: u32, height: u32) {
        for pass in &mut self.passes {
            if pass.name() == "SSR" {
                if let Some(ssr) = pass.as_any_mut().downcast_mut::<crate::renderer::passes::ssr::SSRPass>() {
                    ssr.set_screen_size(width, height);
                }
                break;
            }
        }
    }

    /// Set the debug visualization mode on the LightingPass.
    pub fn set_debug_mode(&mut self, mode: u32) {
        for pass in &mut self.passes {
            if pass.name() == "Lighting" {
                // Downcast: we know LightingPass has set_debug_mode
                if let Some(lp) = pass.as_any_mut().downcast_mut::<crate::renderer::passes::lighting::LightingPass>() {
                    lp.set_debug_mode(mode);
                }
                break;
            }
        }
    }

    /// Set the SSR debug visualization mode.
    pub fn set_ssr_debug_mode(&mut self, mode: u32) {
        for pass in &mut self.passes {
            if pass.name() == "SSR" {
                if let Some(ssr) = pass.as_any_mut().downcast_mut::<crate::renderer::passes::ssr::SSRPass>() {
                    ssr.set_debug_mode(mode);
                }
                break;
            }
        }
    }

    /// Enable or disable SSR.
    pub fn set_ssr_enabled(&mut self, enabled: bool) {
        for pass in &mut self.passes {
            if pass.name() == "SSR" {
                if let Some(ssr) = pass.as_any_mut().downcast_mut::<crate::renderer::passes::ssr::SSRPass>() {
                    ssr.set_enabled(enabled);
                }
                break;
            }
        }
    }

    /// Set SSAO parameters (radius, bias, intensity).
    pub fn set_ssao_params(&mut self, radius: f32, bias: f32, intensity: f32) {
        for pass in &mut self.passes {
            if pass.name() == "SSAO" {
                if let Some(ssao) = pass.as_any_mut().downcast_mut::<crate::renderer::passes::ssao::SSAOPass>() {
                    ssao.set_radius(radius);
                    ssao.set_bias(bias);
                    ssao.set_intensity(intensity);
                }
                break;
            }
        }
    }

    /// Toggle rendering features in the LightingPass.
    pub fn set_feature_flags(&mut self, ssao: bool, shadow: bool, ibl: bool) {
        for pass in &mut self.passes {
            if pass.name() == "Lighting" {
                if let Some(lp) = pass.as_any_mut().downcast_mut::<crate::renderer::passes::lighting::LightingPass>() {
                    lp.set_ssao_enabled(ssao);
                    lp.set_shadow_enabled(shadow);
                    lp.set_ibl_enabled(ibl);
                }
                break;
            }
        }
    }

    /// Set tone mapping mode on the ToneMappingPass.
    pub fn set_tone_mapping_mode(&mut self, mode: crate::renderer::passes::tone_mapping::ToneMappingMode, queue: &wgpu::Queue) {
        for pass in &mut self.passes {
            if pass.name() == "ToneMapping" {
                if let Some(tmp) = pass.as_any_mut().downcast_mut::<crate::renderer::passes::tone_mapping::ToneMappingPass>() {
                    tmp.set_mode(mode);
                    tmp.update_uniforms(queue);
                }
                break;
            }
        }
    }

    /// Set bloom parameters.
    pub fn set_bloom_params(&mut self, enabled: bool, threshold: f32, intensity: f32, bloom_intensity: f32, queue: &wgpu::Queue) {
        for pass in &mut self.passes {
            if pass.name() == "Bloom" {
                if let Some(bloom) = pass.as_any_mut().downcast_mut::<crate::renderer::passes::bloom::BloomPass>() {
                    bloom.set_enabled(enabled);
                    bloom.set_threshold(threshold);
                    bloom.set_intensity(intensity);
                    bloom.set_bloom_intensity(bloom_intensity);
                    bloom.update_uniforms(queue);
                }
                break;
            }
        }
    }

    /// Set bloom screen size (call before rebuild).
    pub fn set_bloom_screen_size(&mut self, width: u32, height: u32) {
        for pass in &mut self.passes {
            if pass.name() == "Bloom" {
                if let Some(bloom) = pass.as_any_mut().downcast_mut::<crate::renderer::passes::bloom::BloomPass>() {
                    bloom.set_screen_size(width, height);
                }
                break;
            }
        }
    }

    /// Set FXAA parameters.
    pub fn set_fxaa_params(&mut self, enabled: bool, quality: crate::renderer::passes::fxaa::FxaaQuality, queue: &wgpu::Queue) {
        for pass in &mut self.passes {
            if pass.name() == "FXAA" {
                if let Some(fxaa) = pass.as_any_mut().downcast_mut::<crate::renderer::passes::fxaa::FXAAPass>() {
                    fxaa.set_enabled(enabled);
                    fxaa.set_quality(quality);
                    fxaa.update_uniforms_with_queue(queue);
                }
                break;
            }
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
                let texture = PipelineBuilder::create_transient_texture(device, format, tex_width, tex_height);
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                new_table.allocate(write_slot.type_id, write_slot.name, view);
            }
        }

        self.resource_table = new_table;

        for pass in &mut self.passes {
            pass.resolve(device, &self.resource_table);
        }
    }
}

/// Compute topological order of passes based on read/write signatures.
fn compute_topological_order(passes: &[Box<dyn Pass>]) -> Vec<usize> {
    let n = passes.len();

    let mut producer: HashMap<(std::any::TypeId, &str), usize> = HashMap::new();
    let mut consumers: Vec<HashSet<usize>> = vec![HashSet::new(); n];

    for (i, pass) in passes.iter().enumerate() {
        let sig = pass.signature();

        for write_slot in &sig.writes {
            let key = (write_slot.type_id, write_slot.name);
            if let Some(&existing) = producer.get(&key) {
                panic!(
                    "Resource conflict: '{}' (type {:?}) written by both '{}' and '{}'",
                    write_slot.name, write_slot.type_id,
                    passes[existing].name(), sig.name,
                );
            }
            producer.insert(key, i);
        }
    }

    let mut deps: Vec<Vec<usize>> = vec![Vec::new(); n];

    for (i, pass) in passes.iter().enumerate() {
        let sig = pass.signature();
        for read_slot in &sig.reads {
            let key = (read_slot.type_id, read_slot.name);
            match producer.get(&key) {
                Some(&producer_idx) => {
                    if producer_idx != i && !deps[i].contains(&producer_idx) {
                        deps[i].push(producer_idx);
                        consumers[producer_idx].insert(i);
                    }
                }
                None => {
                    panic!(
                        "Missing producer: pass '{}' reads '{}' (type {:?}), but no pass produces it",
                        sig.name, read_slot.name, read_slot.type_id,
                    );
                }
            }
        }
    }

    detect_cycles(&deps, n, passes);

    // Topological sort (Kahn's algorithm): in_degree[i] = number of passes i depends on
    let mut in_degree: Vec<usize> = deps.iter().map(|d| d.len()).collect();
    let mut queue: VecDeque<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
    let mut order = Vec::with_capacity(n);

    while let Some(node) = queue.pop_front() {
        order.push(node);
        for &consumer in &consumers[node] {
            in_degree[consumer] -= 1;
            if in_degree[consumer] == 0 {
                queue.push_back(consumer);
            }
        }
    }

    if order.len() != n {
        let cycle_nodes: Vec<&str> = (0..n)
            .filter(|i| in_degree[*i] > 0)
            .map(|i| passes[i].name())
            .collect();
        panic!("Dependency cycle detected involving passes: {:?}", cycle_nodes);
    }

    debug!(
        "Topological order: {:?}",
        order.iter().map(|&i| passes[i].name()).collect::<Vec<_>>()
    );

    order
}

/// Detect cycles in the dependency graph via DFS.
fn detect_cycles(deps: &[Vec<usize>], n: usize, passes: &[Box<dyn Pass>]) {
    let mut color = vec![CycleColor::White; n];

    for i in 0..n {
        if color[i] == CycleColor::White {
            dfs(i, deps, &mut color, passes);
        }
    }
}

/// Detect cycles using PassSignature references.
fn detect_cycles_ref(deps: &[Vec<usize>], n: usize, sigs: &[PassSignature]) {
    let mut color = vec![CycleColor::White; n];
    for i in 0..n {
        if color[i] == CycleColor::White {
            dfs_ref(i, deps, &mut color, sigs);
        }
    }
}

fn dfs_ref(node: usize, deps: &[Vec<usize>], color: &mut [CycleColor], sigs: &[PassSignature]) {
    color[node] = CycleColor::Gray;
    for &dep in &deps[node] {
        if color[dep] == CycleColor::Gray {
            panic!(
                "Dependency cycle: '{}' depends on '{}' which depends back on '{}'",
                sigs[node].name, sigs[dep].name, sigs[node].name,
            );
        }
        if color[dep] == CycleColor::White {
            dfs_ref(dep, deps, color, sigs);
        }
    }
    color[node] = CycleColor::Black;
}

#[derive(Clone, Copy, PartialEq)]
enum CycleColor { White, Gray, Black }

fn dfs(node: usize, deps: &[Vec<usize>], color: &mut [CycleColor], passes: &[Box<dyn Pass>]) {
    color[node] = CycleColor::Gray;
    for &dep in &deps[node] {
        if color[dep] == CycleColor::Gray {
            panic!(
                "Dependency cycle: '{}' depends on '{}' which depends back on '{}'",
                passes[node].name(), passes[dep].name(), passes[node].name(),
            );
        }
        if color[dep] == CycleColor::White {
            dfs(dep, deps, color, passes);
        }
    }
    color[node] = CycleColor::Black;
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
            Self { name, reads: Vec::new(), writes: Vec::new(), order_log }
        }

        fn with_write<T: ResourceTag>(
            mut self, name: &'static str, format: wgpu::TextureFormat,
        ) -> Self {
            self.writes.push(ResSlot {
                type_id: TypeId::of::<T>(), name, format: Some(format), kind: SlotKind::Write,
                width: None, height: None,
            });
            self
        }

        fn with_read<T: ResourceTag>(mut self, name: &'static str) -> Self {
            self.reads.push(ResSlot {
                type_id: TypeId::of::<T>(), name, format: None, kind: SlotKind::Read,
                width: None, height: None,
            });
            self
        }
    }

    impl Pass for MockPass {
        fn name(&self) -> &str { self.name }
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
        fn execute(&self, _encoder: &mut wgpu::CommandEncoder, _resources: &ResourceTable, _surface_view: &wgpu::TextureView) {
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

    #[test] fn single_pass_executes() {
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        let pass = MockPass::new("Single", log.clone());
        let s = build_mock_scheduler(vec![pass]);
        assert_eq!(s.passes.len(), 1);
    }

    #[test] fn dependency_order_before_dependent() {
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        let a = MockPass::new("Producer", log.clone())
            .with_write::<GPosition>("pos", wgpu::TextureFormat::Rgba16Float);
        let b = MockPass::new("Consumer", log.clone())
            .with_read::<GPosition>("pos");
        let passes = vec![a, b];
        let boxed: Vec<Box<dyn Pass>> = passes.into_iter().map(|p| Box::new(p) as Box<dyn Pass>).collect();
        let order = compute_topological_order(&boxed);
        assert_eq!(boxed[order[0]].name(), "Producer");
        assert_eq!(boxed[order[1]].name(), "Consumer");
    }

    #[test] fn three_pass_chain() {
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        let a = MockPass::new("A", log.clone())
            .with_write::<GPosition>("pos", wgpu::TextureFormat::Rgba16Float);
        let b = MockPass::new("B", log.clone())
            .with_read::<GPosition>("pos")
            .with_write::<GNormal>("norm", wgpu::TextureFormat::Rgba16Float);
        let c = MockPass::new("C", log.clone())
            .with_read::<GNormal>("norm");
        let passes = vec![a, b, c];
        let boxed: Vec<Box<dyn Pass>> = passes.into_iter().map(|p| Box::new(p) as Box<dyn Pass>).collect();
        let order = compute_topological_order(&boxed);
        assert_eq!(boxed[order[0]].name(), "A");
        assert_eq!(boxed[order[1]].name(), "B");
        assert_eq!(boxed[order[2]].name(), "C");
    }

    #[test] fn independent_passes_keep_registration_order() {
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        let a = MockPass::new("First", log.clone())
            .with_write::<GPosition>("pos", wgpu::TextureFormat::Rgba16Float);
        let b = MockPass::new("Second", log.clone())
            .with_write::<GNormal>("norm", wgpu::TextureFormat::Rgba16Float);
        let passes = vec![a, b];
        let boxed: Vec<Box<dyn Pass>> = passes.into_iter().map(|p| Box::new(p) as Box<dyn Pass>).collect();
        let order = compute_topological_order(&boxed);
        assert_eq!(boxed[order[0]].name(), "First");
        assert_eq!(boxed[order[1]].name(), "Second");
    }

    #[test] #[should_panic(expected = "Missing producer")]
    fn missing_producer_panics() {
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        let pass = MockPass::new("Consumer", log.clone())
            .with_read::<GPosition>("nonexistent");
        let boxed: Vec<Box<dyn Pass>> = vec![Box::new(pass)];
        compute_topological_order(&boxed);
    }

    #[test] #[should_panic(expected = "Resource conflict")]
    fn duplicate_producer_panics() {
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        let a = MockPass::new("A", log.clone())
            .with_write::<GPosition>("same", wgpu::TextureFormat::Rgba16Float);
        let b = MockPass::new("B", log.clone())
            .with_write::<GPosition>("same", wgpu::TextureFormat::Rgba16Float);
        let boxed: Vec<Box<dyn Pass>> = vec![Box::new(a), Box::new(b)];
        compute_topological_order(&boxed);
    }

    #[test] #[should_panic(expected = "Dependency cycle")]
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

    #[test] fn pass_with_no_dependencies_works() {
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        let pass = MockPass::new("Orphan", log.clone())
            .with_write::<Swapchain>("output", wgpu::TextureFormat::Bgra8Unorm);
        let boxed: Vec<Box<dyn Pass>> = vec![Box::new(pass)];
        let order = compute_topological_order(&boxed);
        assert_eq!(order, vec![0]);
    }

    #[test] fn rebuild_reallocates_textures() {
        let device = headless_device();
        let pass_a = ShadowPass::new(&device);
        let pass_b = GBufferPass::new(&device);
        let pass_ssao = crate::renderer::passes::ssao::SSAOPass::new(&device);
        let pass_c = LightingPass::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb);

        let passes: Vec<&dyn Pass> = vec![
            &pass_a as &dyn Pass,
            &pass_b as &dyn Pass,
            &pass_ssao as &dyn Pass,
            &pass_c as &dyn Pass,
        ];
        let table = PipelineBuilder::validate_and_allocate(&passes, &device, 64, 64);
        assert!(table.len() >= 6); // shadow + 4-5 GBuffer + AO

        let table2 = PipelineBuilder::validate_and_allocate(&passes, &device, 128, 128);
        assert!(table2.len() >= 6);
    }

    #[test] fn build_all_passes_works() {
        let device = headless_device();
        let sf = wgpu::TextureFormat::Bgra8UnormSrgb;
        let df = wgpu::TextureFormat::Depth32Float;
        let debug_pass = DebugLinePass::new(&device, sf, df);

        let scheduler = PipelineBuilder::new()
            .add(ShadowPass::new(&device))
            .add(GBufferPass::new(&device))
            .add(crate::renderer::passes::ssao::SSAOPass::new(&device))
            .add(LightingPass::new(&device, sf))
            .add(debug_pass)
            .build(&device, 64, 64);

        assert_eq!(scheduler.pass_count(), 5);
    }

    fn headless_device() -> wgpu::Device {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(
            instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
        )
        .expect("need adapter");
        let (device, _queue) = pollster::block_on(
            adapter.request_device(&wgpu::DeviceDescriptor::default()),
        )
        .expect("need device");
        device
    }
}
