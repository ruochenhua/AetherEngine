//! Pipeline builder: collects passes, topologically sorts, allocates transient textures.
//!
//! `PipelineBuilder` collects passes with their signatures, topologically sorts
//! the dependency graph, allocates transient textures, resolves passes, and
//! produces a `Scheduler`.
//!
//! ## Example
//!
//! ```rust,ignore
//! let ctx = InitContext {
//!     device,
//!     queue,
//!     surface_format,
//!     depth_format,
//!     width,
//!     height,
//!     ibl_resources: Some(ibl),
//! };
//! let scheduler = PipelineBuilder::new()
//!     .add_pass(GBufferPass::init(&ctx))
//!     .add_pass(LightingPass::init(&ctx))
//!     .add_pass(DebugLinePass::init(&ctx))
//!     .build(device, width, height);
//!
//! // Per frame:
//! scheduler.execute_all(&mut encoder, &surface_view);
//! ```

use crate::renderer::pass::{Pass, PassSignature};
use crate::renderer::resource_table::ResourceTable;
use crate::renderer::scheduler::Scheduler;
use std::any::TypeId;
use std::collections::{HashMap, VecDeque};
use thiserror::Error;
use tracing::debug;

/// Errors that can occur while building a render pipeline.
#[derive(Debug, Error)]
pub enum PipelineBuildError {
    /// A pass reads a resource that no pass produces.
    #[error("missing producer: pass '{pass}' reads '{resource}' but no pass produces it")]
    MissingProducer {
        /// Name of the pass with the unresolved read.
        pass: String,
        /// Name of the resource that has no producer.
        resource: String,
    },

    /// A dependency cycle was detected between passes.
    #[error("dependency cycle detected involving passes: {passes:?}")]
    DependencyCycle {
        /// Names of the passes involved in the cycle.
        passes: Vec<String>,
    },

    /// Topological sorting did not visit every pass.
    #[error("topological sort incomplete: visited {visited}/{total} passes. missing: {missing:?}")]
    TopologicalSortIncomplete {
        /// Number of passes that were ordered.
        visited: usize,
        /// Total number of passes in the pipeline.
        total: usize,
        /// Names of the passes that were not ordered.
        missing: Vec<String>,
    },
}

/// Builds a render pipeline from individual passes.
pub struct PipelineBuilder {
    passes: Vec<Box<dyn Pass>>,
}

impl Default for PipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineBuilder {
    /// Create a new empty pipeline builder.
    pub fn new() -> Self {
        Self { passes: Vec::new() }
    }

    /// Add a pass to the pipeline.
    pub fn add_pass<P: Pass + 'static>(mut self, pass: P) -> Self {
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
    ) -> Result<ResourceTable, PipelineBuildError> {
        let boxed: Vec<Box<dyn Pass>> = Vec::new(); // not needed for validation

        // Build signatures from the slice of pass references
        let n = passes.len();
        let mut sigs: Vec<PassSignature> = Vec::with_capacity(n);
        for pass in passes {
            sigs.push(pass.signature());
        }

        // Build producer map and dependency edges. Multiple sequential writers
        // to the same resource are allowed (e.g. GBufferPass + TerrainPass).
        let mut producer: HashMap<(TypeId, &str), usize> = HashMap::new();
        let mut deps: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, sig) in sigs.iter().enumerate() {
            for write_slot in &sig.writes {
                let key = (write_slot.type_id, write_slot.name);
                if let Some(&existing) = producer.get(&key) {
                    if existing != i && !deps[i].contains(&existing) {
                        deps[i].push(existing);
                    }
                }
                producer.insert(key, i);
            }
        }

        // Add read dependencies.
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
                        return Err(PipelineBuildError::MissingProducer {
                            pass: sig.name.to_string(),
                            resource: read_slot.name.to_string(),
                        });
                    }
                }
            }
        }

        // Detect cycles
        if let Some(cycle) = detect_cycles_ref(&deps, n, &sigs) {
            return Err(PipelineBuildError::DependencyCycle { passes: cycle });
        }

        // Topological sort
        let order = topological_sort(&deps, n, |i| sigs[i].name.to_string())?;

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
                let layers = write_slot.layers.unwrap_or(1);
                let texture =
                    Self::create_transient_texture(device, format, tex_width, tex_height, layers);
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                resource_table.allocate_with_texture(
                    write_slot.type_id,
                    write_slot.name,
                    texture,
                    view,
                );
            }
        }

        let _ = boxed;
        Ok(resource_table)
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
    ///
    /// Multiple passes may write to the same `(type, name)` resource; they are
    /// treated as sequential writers and ordered by registration order.
    pub fn build(
        mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> Result<Scheduler, PipelineBuildError> {
        let order = compute_topological_order(&self.passes)?;

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
                    let layers = write_slot.layers.unwrap_or(1);
                    let texture = Self::create_transient_texture(
                        device, format, tex_width, tex_height, layers,
                    );
                    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                    resource_table.allocate_with_texture(
                        write_slot.type_id,
                        write_slot.name,
                        texture,
                        view,
                    );
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

        // Resolve each pass with the resource table
        for pass in &mut ordered {
            pass.resolve(device, &resource_table);
        }

        Ok(Scheduler {
            passes: ordered,
            resource_table,
        })
    }

    /// Create a transient texture for intermediate pass outputs.
    pub(crate) fn create_transient_texture(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        layers: u32,
    ) -> wgpu::Texture {
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST;
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Transient Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: layers,
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

/// Compute topological order of passes based on read/write signatures.
pub(crate) fn compute_topological_order(
    passes: &[Box<dyn Pass>],
) -> Result<Vec<usize>, PipelineBuildError> {
    let n = passes.len();

    let mut producer: HashMap<(std::any::TypeId, &str), usize> = HashMap::new();
    let mut deps: Vec<Vec<usize>> = vec![Vec::new(); n];

    for (i, pass) in passes.iter().enumerate() {
        let sig = pass.signature();

        for write_slot in &sig.writes {
            let key = (write_slot.type_id, write_slot.name);
            if let Some(&existing) = producer.get(&key) {
                // Allow multiple sequential writers to the same resource (e.g.
                // GBufferPass and TerrainPass both write to GPosition). The
                // earlier writer becomes a dependency of the later one.
                if existing != i && !deps[i].contains(&existing) {
                    deps[i].push(existing);
                }
            }
            producer.insert(key, i);
        }
    }

    for (i, pass) in passes.iter().enumerate() {
        let sig = pass.signature();
        for read_slot in &sig.reads {
            let key = (read_slot.type_id, read_slot.name);
            match producer.get(&key) {
                Some(&producer_idx) => {
                    if producer_idx != i && !deps[i].contains(&producer_idx) {
                        deps[i].push(producer_idx);
                    }
                }
                None => {
                    return Err(PipelineBuildError::MissingProducer {
                        pass: sig.name.to_string(),
                        resource: read_slot.name.to_string(),
                    });
                }
            }
        }
    }

    if let Some(cycle) = detect_cycles(&deps, n, passes) {
        return Err(PipelineBuildError::DependencyCycle { passes: cycle });
    }

    let order = topological_sort(&deps, n, |i| passes[i].name().to_string())?;

    debug!(
        "Topological order: {:?}",
        order.iter().map(|&i| passes[i].name()).collect::<Vec<_>>()
    );

    Ok(order)
}

/// Kahn's topological sort.
///
/// `name` maps a node index to a human-readable pass name for error reporting.
fn topological_sort<F>(
    deps: &[Vec<usize>],
    n: usize,
    name: F,
) -> Result<Vec<usize>, PipelineBuildError>
where
    F: Fn(usize) -> String,
{
    let mut consumers: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, deps_i) in deps.iter().enumerate() {
        for &dep in deps_i {
            consumers[dep].push(i);
        }
    }

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
        let missing: Vec<String> = (0..n).filter(|i| !order.contains(i)).map(name).collect();
        return Err(PipelineBuildError::TopologicalSortIncomplete {
            visited: order.len(),
            total: n,
            missing,
        });
    }

    Ok(order)
}

/// Detect cycles in the dependency graph via DFS.
///
/// Returns the list of pass names involved in a cycle, if any.
fn detect_cycles(deps: &[Vec<usize>], n: usize, passes: &[Box<dyn Pass>]) -> Option<Vec<String>> {
    let mut color = vec![CycleColor::White; n];

    for i in 0..n {
        if color[i] == CycleColor::White {
            if let Some(cycle) = dfs(i, deps, &mut color, passes) {
                return Some(cycle);
            }
        }
    }
    None
}

/// Detect cycles using PassSignature references (for validate_and_allocate).
///
/// Returns the list of pass names involved in a cycle, if any.
fn detect_cycles_ref(deps: &[Vec<usize>], n: usize, sigs: &[PassSignature]) -> Option<Vec<String>> {
    let mut color = vec![CycleColor::White; n];
    for i in 0..n {
        if color[i] == CycleColor::White {
            if let Some(cycle) = dfs_ref(i, deps, &mut color, sigs) {
                return Some(cycle);
            }
        }
    }
    None
}

fn dfs_ref(
    node: usize,
    deps: &[Vec<usize>],
    color: &mut [CycleColor],
    sigs: &[PassSignature],
) -> Option<Vec<String>> {
    color[node] = CycleColor::Gray;
    for &dep in &deps[node] {
        if color[dep] == CycleColor::Gray {
            return Some(vec![
                sigs[node].name.to_string(),
                sigs[dep].name.to_string(),
                sigs[node].name.to_string(),
            ]);
        }
        if color[dep] == CycleColor::White {
            if let Some(cycle) = dfs_ref(dep, deps, color, sigs) {
                return Some(cycle);
            }
        }
    }
    color[node] = CycleColor::Black;
    None
}

#[derive(Clone, Copy, PartialEq)]
enum CycleColor {
    White,
    Gray,
    Black,
}

fn dfs(
    node: usize,
    deps: &[Vec<usize>],
    color: &mut [CycleColor],
    passes: &[Box<dyn Pass>],
) -> Option<Vec<String>> {
    color[node] = CycleColor::Gray;
    for &dep in &deps[node] {
        if color[dep] == CycleColor::Gray {
            return Some(vec![
                passes[node].name().to_string(),
                passes[dep].name().to_string(),
                passes[node].name().to_string(),
            ]);
        }
        if color[dep] == CycleColor::White {
            if let Some(cycle) = dfs(dep, deps, color, passes) {
                return Some(cycle);
            }
        }
    }
    color[node] = CycleColor::Black;
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::headless_device_queue;
    use crate::renderer::pass::{InitContext, ResSlot, SlotKind};
    use crate::renderer::resource::*;
    use std::sync::{Arc, Mutex};

    /// Mock pass mirroring the scheduler.rs test pattern, but recording
    /// `resolve()` calls — the builder (not the scheduler) is responsible for
    /// resolving passes during `build()`.
    struct MockPass {
        name: &'static str,
        reads: Vec<ResSlot>,
        writes: Vec<ResSlot>,
        resolve_log: Arc<Mutex<Vec<String>>>,
    }

    impl MockPass {
        fn new(name: &'static str, resolve_log: Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                name,
                reads: Vec::new(),
                writes: Vec::new(),
                resolve_log,
            }
        }

        fn with_write<T: ResourceTag>(mut self, format: wgpu::TextureFormat) -> Self {
            self.writes.push(ResSlot {
                type_id: TypeId::of::<T>(),
                name: T::NAME,
                format: Some(format),
                kind: SlotKind::Write,
                width: None,
                height: None,
                layers: None,
            });
            self
        }

        fn with_write_sized<T: ResourceTag>(
            mut self,
            format: wgpu::TextureFormat,
            width: u32,
            height: u32,
        ) -> Self {
            self.writes.push(ResSlot {
                type_id: TypeId::of::<T>(),
                name: T::NAME,
                format: Some(format),
                kind: SlotKind::Write,
                width: Some(width),
                height: Some(height),
                layers: None,
            });
            self
        }

        fn with_write_array<T: ResourceTag>(
            mut self,
            format: wgpu::TextureFormat,
            width: u32,
            height: u32,
            layers: u32,
        ) -> Self {
            self.writes.push(ResSlot {
                type_id: TypeId::of::<T>(),
                name: T::NAME,
                format: Some(format),
                kind: SlotKind::Write,
                width: Some(width),
                height: Some(height),
                layers: Some(layers),
            });
            self
        }

        fn with_read<T: ResourceTag>(mut self) -> Self {
            self.reads.push(ResSlot {
                type_id: TypeId::of::<T>(),
                name: T::NAME,
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
        fn init(_ctx: &InitContext) -> Self {
            panic!("MockPass does not support init()")
        }
        fn resolve(&mut self, _device: &wgpu::Device, _resources: &ResourceTable) {
            self.resolve_log
                .lock()
                .unwrap()
                .push(self.name.to_string());
        }
        fn execute(
            &self,
            _encoder: &mut wgpu::CommandEncoder,
            _resources: &ResourceTable,
            _surface_view: &wgpu::TextureView,
        ) {
        }
    }

    /// A read with no producing pass must fail `build()` with `MissingProducer`
    /// naming the offending pass and resource.
    #[test]
    fn build_missing_producer_reports_pass_and_resource() {
        let Some((device, _queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let log = Arc::new(Mutex::new(Vec::new()));
        let consumer = MockPass::new("Consumer", log).with_read::<GPosition>();

        let err = PipelineBuilder::new()
            .add_pass(consumer)
            .build(&device, 64, 64)
            .err()
            .expect("a read without a producer must fail the build");

        match &err {
            PipelineBuildError::MissingProducer { pass, resource } => {
                assert_eq!(pass, "Consumer");
                assert_eq!(resource, GPosition::NAME);
            }
            other => panic!("expected MissingProducer, got: {other}"),
        }
        assert_eq!(
            err.to_string(),
            "missing producer: pass 'Consumer' reads 'gbuffer_position' but no pass produces it"
        );
    }

    /// A cyclic read/write graph must fail `build()` with `DependencyCycle`
    /// naming the passes involved in the cycle.
    #[test]
    fn build_dependency_cycle_reports_involved_passes() {
        let Some((device, _queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let log = Arc::new(Mutex::new(Vec::new()));
        let a = MockPass::new("A", log.clone())
            .with_write::<GPosition>(wgpu::TextureFormat::Rgba16Float)
            .with_read::<GNormal>();
        let b = MockPass::new("B", log)
            .with_write::<GNormal>(wgpu::TextureFormat::Rgba16Float)
            .with_read::<GPosition>();

        let err = PipelineBuilder::new()
            .add_pass(a)
            .add_pass(b)
            .build(&device, 64, 64)
            .err()
            .expect("a cyclic pass graph must fail the build");

        match &err {
            PipelineBuildError::DependencyCycle { passes } => {
                assert!(
                    passes.iter().any(|p| p == "A"),
                    "cycle report should name pass A, got: {passes:?}"
                );
                assert!(
                    passes.iter().any(|p| p == "B"),
                    "cycle report should name pass B, got: {passes:?}"
                );
            }
            other => panic!("expected DependencyCycle, got: {other}"),
        }
    }

    /// The legal GBuffer+Terrain scenario: both passes write the same G-Buffer
    /// targets (sequential writers). `build()` must succeed, allocate exactly
    /// one texture per unique resource, and resolve every pass once.
    #[test]
    fn build_sequential_writers_allocates_each_resource_once() {
        let Some((device, _queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let resolve_log = Arc::new(Mutex::new(Vec::new()));

        let gbuffer = MockPass::new("GBuffer", resolve_log.clone())
            .with_write::<GPosition>(wgpu::TextureFormat::Rgba16Float)
            .with_write::<GNormal>(wgpu::TextureFormat::Rgba16Float)
            .with_write::<GAlbedo>(wgpu::TextureFormat::Rgba8Unorm)
            .with_write::<GMaterial>(wgpu::TextureFormat::Rg8Unorm)
            .with_write::<GDepth>(wgpu::TextureFormat::Depth32Float);
        let terrain = MockPass::new("Terrain", resolve_log.clone())
            .with_write::<GPosition>(wgpu::TextureFormat::Rgba16Float)
            .with_write::<GNormal>(wgpu::TextureFormat::Rgba16Float)
            .with_write::<GAlbedo>(wgpu::TextureFormat::Rgba8Unorm)
            .with_write::<GMaterial>(wgpu::TextureFormat::Rg8Unorm)
            .with_write::<GDepth>(wgpu::TextureFormat::Depth32Float);
        let lighting = MockPass::new("Lighting", resolve_log.clone())
            .with_read::<GPosition>()
            .with_read::<GNormal>();

        let scheduler = PipelineBuilder::new()
            .add_pass(gbuffer)
            .add_pass(terrain)
            .add_pass(lighting)
            .build(&device, 64, 64)
            .expect("GBuffer+Terrain sequential double-write is a legal pipeline");

        assert_eq!(scheduler.pass_count(), 3);
        // Two writers per target, but only one texture per unique resource.
        assert_eq!(
            scheduler.resource_table.len(),
            5,
            "each unique (type, name) write must be allocated exactly once"
        );

        // Unsized writes fall back to the build dimensions.
        let pos = scheduler.resource_table.handle::<GPosition>();
        let pos_size = scheduler
            .resource_table
            .texture(pos)
            .expect("transient texture should be owned by the table")
            .size();
        assert_eq!((pos_size.width, pos_size.height), (64, 64));

        // Every pass was resolved exactly once during build.
        let mut resolved = resolve_log.lock().unwrap().clone();
        resolved.sort();
        assert_eq!(resolved, vec!["GBuffer", "Lighting", "Terrain"]);
    }

    /// `validate_and_allocate` reports the same `MissingProducer` error as
    /// `build()`, naming the first unresolved read.
    #[test]
    fn validate_and_allocate_missing_producer_reports_pass_and_resource() {
        let Some((device, _queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let log = Arc::new(Mutex::new(Vec::new()));
        let lighting = MockPass::new("Lighting", log)
            .with_read::<GPosition>()
            .with_read::<GNormal>();
        let passes: Vec<&dyn Pass> = vec![&lighting];

        let err = PipelineBuilder::validate_and_allocate(&passes, &device, 64, 64)
            .err()
            .expect("reads without producers must fail validation");

        match &err {
            PipelineBuildError::MissingProducer { pass, resource } => {
                assert_eq!(pass, "Lighting");
                // The first unresolved read in declaration order is reported.
                assert_eq!(resource, GPosition::NAME);
            }
            other => panic!("expected MissingProducer, got: {other}"),
        }
    }

    /// `validate_and_allocate` detects dependency cycles without taking
    /// ownership of the passes.
    #[test]
    fn validate_and_allocate_dependency_cycle_returns_error() {
        let Some((device, _queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let log = Arc::new(Mutex::new(Vec::new()));
        let a = MockPass::new("A", log.clone())
            .with_write::<GPosition>(wgpu::TextureFormat::Rgba16Float)
            .with_read::<GNormal>();
        let b = MockPass::new("B", log)
            .with_write::<GNormal>(wgpu::TextureFormat::Rgba16Float)
            .with_read::<GPosition>();
        let passes: Vec<&dyn Pass> = vec![&a, &b];

        let err = PipelineBuilder::validate_and_allocate(&passes, &device, 64, 64)
            .err()
            .expect("a cyclic pass graph must fail validation");

        match &err {
            PipelineBuildError::DependencyCycle { passes } => {
                assert!(
                    passes.iter().any(|p| p == "A"),
                    "cycle report should name pass A, got: {passes:?}"
                );
                assert!(
                    passes.iter().any(|p| p == "B"),
                    "cycle report should name pass B, got: {passes:?}"
                );
            }
            other => panic!("expected DependencyCycle, got: {other}"),
        }
    }

    /// Allocation honors explicit per-slot sizes and array layers; unsized
    /// writes fall back to the default screen size. `validate_and_allocate`
    /// only allocates — it must not resolve passes.
    #[test]
    fn validate_and_allocate_honors_explicit_slot_sizes() {
        let Some((device, _queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let resolve_log = Arc::new(Mutex::new(Vec::new()));
        let pass = MockPass::new("Sized", resolve_log.clone())
            .with_write::<GPosition>(wgpu::TextureFormat::Rgba16Float)
            .with_write_sized::<AOTexture>(wgpu::TextureFormat::R8Unorm, 32, 16)
            .with_write_array::<ShadowDepth>(wgpu::TextureFormat::Depth32Float, 128, 128, 4);
        let passes: Vec<&dyn Pass> = vec![&pass];

        let table = PipelineBuilder::validate_and_allocate(&passes, &device, 640, 480)
            .expect("a writes-only pass graph is valid");
        assert_eq!(table.len(), 3);

        // Unsized write falls back to the default (screen) size.
        let pos = table.handle::<GPosition>();
        let pos_size = table.texture(pos).expect("owned texture").size();
        assert_eq!((pos_size.width, pos_size.height), (640, 480));

        // Explicitly sized write keeps its declared dimensions.
        let ao = table.handle::<AOTexture>();
        let ao_size = table.texture(ao).expect("owned texture").size();
        assert_eq!((ao_size.width, ao_size.height), (32, 16));

        // Array write keeps its declared layer count.
        let shadow = table.handle::<ShadowDepth>();
        let shadow_size = table.texture(shadow).expect("owned texture").size();
        assert_eq!(shadow_size.depth_or_array_layers, 4);

        assert!(
            resolve_log.lock().unwrap().is_empty(),
            "validate_and_allocate must not resolve passes"
        );
    }
}
