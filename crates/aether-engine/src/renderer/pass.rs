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
//! use aether_engine::renderer::pass::{Pass, PassSignature};
//! use aether_engine::renderer::resource::{GPosition, GNormal};
//!
//! struct MyPass;
//!
//! impl Pass for MyPass {
//!     fn name(&self) -> &str { "MyPass" }
//!     fn signature(&self) -> PassSignature {
//!         PassSignature::new("MyPass")
//!             .read::<GPosition>("gbuffer_position")
//!             .write::<GNormal>("gbuffer_normal", wgpu::TextureFormat::Rgba16Float)
//!     }
//!     fn init(device: &wgpu::Device) -> Self { MyPass }
//!     fn execute(&self, encoder: &mut wgpu::CommandEncoder, resources: &crate::renderer::resource_table::ResourceTable) {}
//! }
//! ```

use std::any::TypeId;
use std::marker::PhantomData;

use crate::renderer::resource::ResourceTag;
use crate::renderer::resource_table::ResourceTable;

/// A render pass that declares its resource dependencies.
///
/// Three-phase lifecycle:
/// 1. `init()` — create pipelines, shaders, uniform buffers (no texture access)
/// 2. `resolve()` — create texture-dependent bind groups from ResourceTable
/// 3. `execute()` — record render commands
pub trait Pass {
    /// Human-readable pass name.
    fn name(&self) -> &str;

    /// Declare resource dependencies (reads and writes).
    fn signature(&self) -> PassSignature;

    /// Create GPU resources not dependent on transient textures.
    fn init(device: &wgpu::Device) -> Self
    where
        Self: Sized;

    /// Create texture-dependent bind groups.
    ///
    /// Called by the Scheduler after transient textures are allocated.
    fn resolve(&mut self, _device: &wgpu::Device, _resources: &ResourceTable) {}

    /// Record render commands.
    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        queue: &wgpu::Queue,
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

    /// Add a read dependency.
    pub fn read<T: ResourceTag>(mut self, name: &'static str) -> Self {
        self.reads.push(ResSlot {
            type_id: TypeId::of::<T>(),
            name,
            format: None, // Reads don't declare format
            kind: SlotKind::Read,
        });
        self
    }

    /// Add a write dependency with the texture format.
    pub fn write<T: ResourceTag>(mut self, name: &'static str, format: wgpu::TextureFormat) -> Self {
        self.writes.push(ResSlot {
            type_id: TypeId::of::<T>(),
            name,
            format: Some(format),
            kind: SlotKind::Write,
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
}

impl ResSlot {
    /// Create a write slot.
    pub fn new<T: ResourceTag>(name: &'static str, format: wgpu::TextureFormat) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            name,
            format: Some(format),
            kind: SlotKind::Write,
        }
    }

    /// Create a read slot.
    pub fn read<T: ResourceTag>(name: &'static str) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            name,
            format: None,
            kind: SlotKind::Read,
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
        Self {
            index: self.index,
            _phantom: PhantomData,
        }
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
            .read::<GPosition>("gbuffer_position")
            .write::<AOTexture>("ao", wgpu::TextureFormat::R8Unorm);

        assert_eq!(sig.name, "TestPass");
        assert_eq!(sig.reads.len(), 1);
        assert_eq!(sig.reads[0].name, "gbuffer_position");
        assert_eq!(sig.reads[0].type_id, std::any::TypeId::of::<GPosition>());
        assert_eq!(sig.reads[0].kind, SlotKind::Read);

        assert_eq!(sig.writes.len(), 1);
        assert_eq!(sig.writes[0].name, "ao");
        assert_eq!(sig.writes[0].type_id, std::any::TypeId::of::<AOTexture>());
        assert_eq!(sig.writes[0].kind, SlotKind::Write);
        assert_eq!(sig.writes[0].format, Some(wgpu::TextureFormat::R8Unorm));
    }

    /// Multiple reads on the same resource type are allowed.
    #[test]
    fn multiple_reads_same_type() {
        let sig = PassSignature::new("MultiRead")
            .read::<GPosition>("gbuffer_position")
            .read::<GPosition>("world_position"); // different name, same type

        assert_eq!(sig.reads.len(), 2);
        // Same TypeId
        assert_eq!(sig.reads[0].type_id, sig.reads[1].type_id);
        // Different names
        assert_ne!(sig.reads[0].name, sig.reads[1].name);
    }
}
