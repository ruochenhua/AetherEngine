//! Resource table for pass scheduling.
//!
//! Maps `(TypeId, name)` pairs to transient texture views.
//! Passes receive `ResHandle<T>` handles that index into this table.
//!
//! Created by the Scheduler during `build()` and passed to each pass
//! during `resolve()` and `execute()`.

use std::any::TypeId;
use crate::renderer::pass::ResHandle;
use crate::renderer::resource::ResourceTag;

/// Table of transient resources shared between passes.
pub struct ResourceTable {
    /// Texture views indexed by handle index.
    views: Vec<wgpu::TextureView>,
    /// Mapping from (TypeId, name) → index into views.
    mapping: Vec<(TypeId, &'static str)>,
}

impl ResourceTable {
    /// Create an empty resource table.
    pub(crate) fn new() -> Self {
        Self {
            views: Vec::new(),
            mapping: Vec::new(),
        }
    }

    /// Allocate a new resource and return its handle.
    pub(crate) fn allocate(
        &mut self,
        type_id: TypeId,
        name: &'static str,
        view: wgpu::TextureView,
    ) -> usize {
        let index = self.views.len();
        self.mapping.push((type_id, name));
        self.views.push(view);
        index
    }

    /// Get a ResHandle for a declared resource.
    ///
    /// Used by passes during `resolve()` to obtain handles for the resources
    /// they declared in their signature.
    ///
    /// # Panics
    ///
    /// Panics if no resource matches the given type and name.
    pub fn handle<T: ResourceTag>(&self, name: &str) -> ResHandle<T> {
        let type_id = TypeId::of::<T>();
        for (i, &(tid, n)) in self.mapping.iter().enumerate() {
            if tid == type_id && n == name {
                return ResHandle::new(i);
            }
        }
        panic!(
            "Resource '{}' with type {:?} not found in ResourceTable. Available: {:?}",
            name,
            type_id,
            self.mapping.iter().map(|(tid, n)| (tid, n)).collect::<Vec<_>>(),
        );
    }

    /// Get a texture view by handle.
    pub fn get<T: ResourceTag>(&self, handle: ResHandle<T>) -> &wgpu::TextureView {
        &self.views[handle.index]
    }

    /// Number of entries in the table.
    pub fn len(&self) -> usize {
        self.views.len()
    }

    /// Returns true if the table contains no entries.
    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    /// Update a texture view for a resource (used for per-frame swapchain).
    pub fn set_view(&mut self, type_id: TypeId, name: &str, view: wgpu::TextureView) {
        for (i, &(tid, n)) in self.mapping.iter().enumerate() {
            if tid == type_id && n == name {
                self.views[i] = view;
                return;
            }
        }
        panic!("Resource '{}' with type {:?} not found for update", name, type_id);
    }
}

/// Metadata about a resource entry.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct ResourceEntry {
    pub type_id: TypeId,
    pub name: &'static str,
    pub format: wgpu::TextureFormat,
}

impl ResourceEntry {
    #[allow(dead_code)]
    pub fn new(type_id: TypeId, name: &'static str, format: wgpu::TextureFormat) -> Self {
        Self { type_id, name, format }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::resource::*;

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

    fn create_texture_view(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::TextureView {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("test texture"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        texture.create_view(&wgpu::TextureViewDescriptor::default())
    }

    #[test]
    fn alloc_and_retrieve_by_handle() {
        let device = headless_device();
        let mut table = ResourceTable::new();

        let view = create_texture_view(&device, wgpu::TextureFormat::Rgba16Float);
        let idx = table.allocate(TypeId::of::<GPosition>(), "gbuffer_position", view);

        let handle = table.handle::<GPosition>("gbuffer_position");
        assert_eq!(handle.index, idx);

        // Verify we can get the view back
        let _retrieved = table.get(handle);
    }

    #[test]
    fn handle_type_safety() {
        let device = headless_device();
        let mut table = ResourceTable::new();

        let pos_view = create_texture_view(&device, wgpu::TextureFormat::Rgba16Float);
        table.allocate(TypeId::of::<GPosition>(), "pos", pos_view);

        let pos_handle: ResHandle<GPosition> = table.handle("pos");

        // These would fail to compile:
        // let _: ResHandle<GNormal> = table.handle::<GNormal>("pos"); // panic at runtime

        // But getting via wrong type panics at runtime (valid design: typo in name, not type)
        // Getting via correct type succeeds
        let _view = table.get(pos_handle);
    }

    #[test]
    #[should_panic(expected = "not found in ResourceTable")]
    fn missing_handle_panics() {
        let _device = headless_device();
        let table = ResourceTable::new();

        // No resources allocated — handle lookup fails
        let _: ResHandle<GPosition> = table.handle("nonexistent");
    }

    #[test]
    fn multiple_resources() {
        let device = headless_device();
        let mut table = ResourceTable::new();

        let v1 = create_texture_view(&device, wgpu::TextureFormat::Rgba16Float);
        let v2 = create_texture_view(&device, wgpu::TextureFormat::Rgba16Float);
        let v3 = create_texture_view(&device, wgpu::TextureFormat::R8Unorm);

        table.allocate(TypeId::of::<GPosition>(), "pos", v1);
        table.allocate(TypeId::of::<GNormal>(), "norm", v2);
        table.allocate(TypeId::of::<AOTexture>(), "ao", v3);

        assert_eq!(table.len(), 3);

        let pos_h: ResHandle<GPosition> = table.handle("pos");
        let norm_h: ResHandle<GNormal> = table.handle("norm");
        let ao_h: ResHandle<AOTexture> = table.handle("ao");

        assert_ne!(pos_h.index, norm_h.index);
        assert_ne!(pos_h.index, ao_h.index);
    }
}
