use hecs::Entity;
use tracing::trace;

/// The ECS World.
///
/// Container for all entities, components, and resources.
/// Thin wrapper around `hecs::World` with engine-specific utilities.
pub struct World {
    inner: hecs::World,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// Create a new empty world.
    pub fn new() -> Self {
        trace!("Creating new ECS World");
        Self {
            inner: hecs::World::new(),
        }
    }

    /// Spawn a new entity with the given components.
    pub fn spawn<B>(&mut self, bundle: B) -> Entity
    where
        B: hecs::DynamicBundle,
    {
        self.inner.spawn(bundle)
    }

    /// Despawn an entity.
    pub fn despawn(&mut self, entity: Entity) -> Result<(), hecs::NoSuchEntity> {
        self.inner.despawn(entity)
    }

    /// Check if an entity exists.
    pub fn contains(&self, entity: Entity) -> bool {
        self.inner.contains(entity)
    }

    /// Query components immutably.
    pub fn query<Q: hecs::Query>(&self) -> hecs::QueryBorrow<'_, Q> {
        self.inner.query::<Q>()
    }

    /// Query components mutably.
    pub fn query_mut<Q: hecs::Query>(&mut self) -> hecs::QueryMut<'_, Q> {
        self.inner.query_mut::<Q>()
    }

    /// Insert components into an existing entity.
    pub fn insert<B>(&mut self, entity: Entity, bundle: B) -> Result<(), hecs::NoSuchEntity>
    where
        B: hecs::DynamicBundle,
    {
        self.inner.insert(entity, bundle)
    }

    /// Remove components from an entity.
    pub fn remove<T: hecs::Bundle + 'static>(
        &mut self,
        entity: Entity,
    ) -> Result<T, hecs::ComponentError> {
        self.inner.remove::<T>(entity)
    }

    /// Get the number of entities.
    pub fn len(&self) -> u32 {
        self.inner.len()
    }

    /// Check if the world is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Clear all entities.
    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

impl std::ops::Deref for World {
    type Target = hecs::World;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for World {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
