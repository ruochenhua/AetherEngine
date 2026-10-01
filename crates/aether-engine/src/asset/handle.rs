/// Unique handle to an asset generation.
pub struct Handle<T> {
    slot: u32,
    generation: u32,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> Copy for Handle<T> {}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> std::fmt::Debug for Handle<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Handle")
            .field("slot", &self.slot)
            .field("generation", &self.generation)
            .finish()
    }
}

impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.slot == other.slot && self.generation == other.generation
    }
}

impl<T> Eq for Handle<T> {}

impl<T> std::hash::Hash for Handle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.slot, state);
        std::hash::Hash::hash(&self.generation, state);
    }
}

impl<T> Handle<T> {
    /// Create a new handle from a packed legacy ID.
    pub fn new(id: u64) -> Self {
        Self {
            slot: id as u32,
            generation: (id >> 32) as u32,
            _phantom: std::marker::PhantomData,
        }
    }

    pub(crate) fn from_parts(slot: u32, generation: u32) -> Self {
        Self {
            slot,
            generation,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Return the stable slot index.
    pub fn slot(&self) -> u32 {
        self.slot
    }

    /// Return the asset generation captured by this handle.
    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// Get the underlying packed ID.
    pub fn id(&self) -> u64 {
        (u64::from(self.generation) << 32) | u64::from(self.slot)
    }
}
