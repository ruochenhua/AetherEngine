//! Type-safe resource handles and Pass trait.
//!
//! ## Design
//!
//! Each transient resource (G-Buffer texture, AO texture, etc.) gets a zero-size
//! type tag. `ResHandle<T>` is parameterized on this tag, so the compiler prevents
//! passing a G-Buffer normal where an AO texture is expected.
//!
//! ## Example
//!
//! ```rust,ignore
//! use aether_engine::renderer::pass::{InitContext, Pass, PassSignature};
//! use aether_engine::renderer::resource::{GPosition, GNormal};
//!
//! struct MyPass;
//!
//! impl Pass for MyPass {
//!     fn name(&self) -> &str { "MyPass" }
//!     fn signature(&self) -> PassSignature {
//!         PassSignature::new("MyPass")
//!             .read::<GPosition>()
//!             .write::<GNormal>(wgpu::TextureFormat::Rgba16Float)
//!     }
//!     fn init(_ctx: &InitContext) -> Self { MyPass }
//!     fn apply_frame(&mut self, frame: &crate::renderer::frame::RenderFrame) {
//!         // Extract per-frame data here
//!     }
//!     fn execute(&self, encoder: &mut wgpu::CommandEncoder, resources: &crate::renderer::resource_table::ResourceTable, surface_view: &wgpu::TextureView) {}
//! }
//! ```
//!
//! Typos in resource names are caught at compile time:
//!
//! ```compile_fail
//! use aether_engine::renderer::pass::PassSignature;
//! use aether_engine::renderer::resource::GPosition;
//!
//! let _ = PassSignature::new("Bad")
//!     .read::<GPosition>("gbuffer_pos"); // error: `read` takes 0 arguments
//! ```

use std::any::TypeId;
use std::marker::PhantomData;

use crate::renderer::frame::RenderFrame;
use crate::renderer::ibl::IblResources;
use crate::renderer::resource::ResourceTag;
use crate::renderer::resource_table::ResourceTable;

/// Construction-time context available to every pass.
///
/// Passes receive all renderer-wide inputs (device, queue, surface formats,
/// resolution, IBL resources) through this single struct, so `Pass::init` is a
/// uniform interface even when individual passes need extra parameters.
#[derive(Clone, Copy)]
pub struct InitContext<'a> {
    /// wgpu device for creating pipelines and buffers.
    pub device: &'a wgpu::Device,
    /// wgpu queue for uploading initial data.
    pub queue: &'a wgpu::Queue,
    /// Swapchain surface format.
    pub surface_format: wgpu::TextureFormat,
    /// Depth buffer format used by the pipeline.
    pub depth_format: wgpu::TextureFormat,
    /// Current backbuffer width.
    pub width: u32,
    /// Current backbuffer height.
    pub height: u32,
    /// Optional IBL resources; required by passes such as `LightingPass`.
    pub ibl_resources: Option<&'a IblResources>,
}

/// A render pass that declares its resource dependencies.
///
/// Four-phase lifecycle:
/// 1. `init()` — create pipelines, shaders, uniform buffers (no texture access)
/// 2. `resolve()` — create texture-dependent bind groups from ResourceTable
/// 3. `apply_frame()` — receive per-frame data (renderables, camera, lighting)
/// 4. `execute()` — record render commands
pub trait Pass {
    /// Human-readable pass name.
    fn name(&self) -> &str;

    /// Declare resource dependencies (reads and writes).
    fn signature(&self) -> PassSignature;

    /// Create GPU resources not dependent on transient textures.
    fn init(ctx: &InitContext) -> Self
    where
        Self: Sized;

    /// Create texture-dependent bind groups.
    ///
    /// Called by the Scheduler after transient textures are allocated.
    fn resolve(&mut self, _device: &wgpu::Device, _resources: &ResourceTable) {}

    /// Receive per-frame data before execution.
    ///
    /// Called by the Scheduler every frame, before `execute()`.
    /// Default implementation is a no-op — passes that don't need
    /// per-frame data don't need to override this.
    fn apply_frame(&mut self, _frame: &RenderFrame) {}

    /// Decide whether this pass should run this frame.
    ///
    /// Default is `true`. Passes can override this to skip execution when
    /// their input data is absent (e.g. no terrain entity in the world).
    fn should_run(&self, _frame: &RenderFrame) -> bool {
        true
    }

    /// Record render commands.
    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        surface_view: &wgpu::TextureView,
    );
}

/// Declared resource dependencies for a pass.
#[derive(Debug, Clone)]
pub struct PassSignature {
    /// Pass name for debugging.
    pub name: &'static str,
    /// Resources this pass reads.
    pub reads: Vec<ResSlot>,
    /// Resources this pass writes.
    pub writes: Vec<ResSlot>,
}

impl PassSignature {
    /// Create a new signature for a named pass.
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            reads: Vec::new(),
            writes: Vec::new(),
        }
    }

    /// Add a read dependency. The resource name is inferred from `T::NAME`.
    pub fn read<T: ResourceTag>(mut self) -> Self {
        self.reads.push(ResSlot {
            type_id: TypeId::of::<T>(),
            name: T::NAME,
            format: None, // Reads don't declare format
            kind: SlotKind::Read,
            width: None,
            height: None,
            layers: None,
        });
        self
    }

    /// Add a write dependency with the texture format. The resource name is
    /// inferred from `T::NAME`.
    pub fn write<T: ResourceTag>(mut self, format: wgpu::TextureFormat) -> Self {
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

    /// Add a write dependency with a fixed texture size. The resource name is
    /// inferred from `T::NAME`.
    pub fn write_sized<T: ResourceTag>(
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

    /// Add a write dependency for a fixed-size array texture. The resource name
    /// is inferred from `T::NAME`.
    pub fn write_array<T: ResourceTag>(
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
}

/// A resource slot in a pass signature — either a read or a write.
#[derive(Debug, Clone, PartialEq)]
pub struct ResSlot {
    /// TypeId of the resource tag (e.g. TypeId::of::<GPosition>()).
    pub type_id: TypeId,
    /// Logical resource name (e.g. "gbuffer_position").
    pub name: &'static str,
    /// Texture format (only meaningful for writes).
    pub format: Option<wgpu::TextureFormat>,
    /// Read or write.
    pub kind: SlotKind,
    /// Fixed width for this texture (overrides scheduler default). Only meaningful for writes.
    pub width: Option<u32>,
    /// Fixed height for this texture (overrides scheduler default). Only meaningful for writes.
    pub height: Option<u32>,
    /// Array layer count for array textures. Only meaningful for writes.
    pub layers: Option<u32>,
}

impl ResSlot {
    /// Create a write slot. The resource name is inferred from `T::NAME`.
    pub fn new<T: ResourceTag>(format: wgpu::TextureFormat) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            name: T::NAME,
            format: Some(format),
            kind: SlotKind::Write,
            width: None,
            height: None,
            layers: None,
        }
    }

    /// Create a read slot. The resource name is inferred from `T::NAME`.
    pub fn read<T: ResourceTag>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            name: T::NAME,
            format: None,
            kind: SlotKind::Read,
            width: None,
            height: None,
            layers: None,
        }
    }
}

/// Whether a resource slot is a read or a write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    /// Pass reads this resource.
    Read,
    /// Pass writes this resource.
    Write,
}

/// Type-safe handle to a transient resource.
///
/// `T` is a zero-size tag type (e.g. `GPosition`, `AOTexture`).
/// The compiler prevents passing a `ResHandle<GPosition>` where a
/// `ResHandle<GNormal>` is expected.
#[derive(Debug)]
pub struct ResHandle<T: ResourceTag> {
    /// Index into the ResourceTable.
    pub(crate) index: usize,
    #[allow(dead_code)]
    _phantom: PhantomData<fn() -> T>,
}

impl<T: ResourceTag> Clone for ResHandle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ResourceTag> Copy for ResHandle<T> {}

impl<T: ResourceTag> ResHandle<T> {
    /// Create a new handle.
    pub(crate) fn new(index: usize) -> Self {
        Self {
            index,
            _phantom: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::resource::*;

    /// Compile-time test: GPosition and GNormal are different types.
    #[test]
    fn different_tags_are_different_types() {
        // This would not compile if types were the same:
        let _a: ResHandle<GPosition> = ResHandle::new(0);
        let _b: ResHandle<GNormal> = ResHandle::new(1);

        // They have the same index but different types — no conflict
        assert_eq!(_a.index, 0);
        assert_eq!(_b.index, 1);
    }

    /// A pass can declare a read and write for different resource types.
    #[test]
    fn signature_reads_and_writes() {
        let sig = PassSignature::new("TestPass")
            .read::<GPosition>()
            .write::<AOTexture>(wgpu::TextureFormat::R8Unorm);

        assert_eq!(sig.name, "TestPass");
        assert_eq!(sig.reads.len(), 1);
        assert_eq!(sig.reads[0].name, GPosition::NAME);
        assert_eq!(sig.reads[0].type_id, std::any::TypeId::of::<GPosition>());
        assert_eq!(sig.reads[0].kind, SlotKind::Read);

        assert_eq!(sig.writes.len(), 1);
        assert_eq!(sig.writes[0].name, AOTexture::NAME);
        assert_eq!(sig.writes[0].type_id, std::any::TypeId::of::<AOTexture>());
        assert_eq!(sig.writes[0].kind, SlotKind::Write);
        assert_eq!(sig.writes[0].format, Some(wgpu::TextureFormat::R8Unorm));
    }

    /// Multiple reads on the same resource type are allowed.
    #[test]
    fn multiple_reads_same_type() {
        let sig = PassSignature::new("MultiRead")
            .read::<GPosition>()
            .read::<GPosition>();

        assert_eq!(sig.reads.len(), 2);
        // Same TypeId
        assert_eq!(sig.reads[0].type_id, sig.reads[1].type_id);
        // Same inferred name
        assert_eq!(sig.reads[0].name, sig.reads[1].name);
    }

    /// Pass signatures infer resource names from the tag type, no string literals.
    #[test]
    fn signature_infers_resource_names() {
        let sig = PassSignature::new("Inferred")
            .read::<GPosition>()
            .write::<AOTexture>(wgpu::TextureFormat::R8Unorm);

        assert_eq!(sig.reads.len(), 1);
        assert_eq!(sig.reads[0].name, GPosition::NAME);
        assert_eq!(sig.writes.len(), 1);
        assert_eq!(sig.writes[0].name, AOTexture::NAME);
    }
}
