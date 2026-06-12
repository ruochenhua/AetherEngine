//! Pipeline builder: collects passes, topologically sorts, allocates transient textures.
//!
//! `PipelineBuilder` collects passes with their signatures, topologically sorts
//! the dependency graph, allocates transient textures, resolves passes, and
//! produces a `Scheduler`.
//!
//! ## Example
//!
//! ```rust,ignore
//! let scheduler = PipelineBuilder::new()
//!     .add_pass(GBufferPass::init(device))
//!     .add_pass(LightingPass::new(device, surface_format))
//!     .add_pass(DebugLinePass::new(device, surface_format, depth_format))
//!     .build(device, width, height);
//!
//! // Per frame:
//! scheduler.execute_all(&mut encoder, &surface_view);
//! ```

use std::any::TypeId;
use std::collections::{HashMap, HashSet, VecDeque};
use crate::renderer::pass::{Pass, PassSignature};
use crate::renderer::resource_table::ResourceTable;
use crate::renderer::scheduler::Scheduler;
use tracing::debug;

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
    ) -> ResourceTable {
        let boxed: Vec<Box<dyn Pass>> = Vec::new(); // not needed for validation

        // Build signatures from the slice of pass references
        let n = passes.len();
        let mut sigs: Vec<PassSignature> = Vec::with_capacity(n);
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

    /// Create a transient texture for intermediate pass outputs.
    pub(crate) fn create_transient_texture(
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

/// Compute topological order of passes based on read/write signatures.
pub(crate) fn compute_topological_order(passes: &[Box<dyn Pass>]) -> Vec<usize> {
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

/// Detect cycles using PassSignature references (for validate_and_allocate).
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
