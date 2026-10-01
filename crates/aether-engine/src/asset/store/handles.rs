//! Typed handle resolution helpers for [`super::AssetStore`].

use super::{AssetError, AssetId, AssetStore};
use crate::asset::store_types::{AssetEntry, LoadStateView};
use crate::asset::{Asset, Handle};
use std::any::TypeId;
use std::path::{Path, PathBuf};
use std::sync::Arc;

impl AssetStore {
    /// Return the configured absolute project root used for path resolution.
    pub fn project_root(&self) -> &Path {
        &self.config.project_root
    }

    /// Build a persistent identity from a typed asset and a project path.
    pub fn asset_id<T: Asset>(&self, path: &Path) -> Result<AssetId, AssetError> {
        let relative = self.compatibility_path(path)?;
        AssetId::from_path(T::KIND, &self.config.project_root, &relative).map_err(Into::into)
    }

    /// Return a handle's persistent identity if it names the active generation.
    pub fn asset_id_for_handle<T: Asset>(&self, handle: Handle<T>) -> Result<AssetId, AssetError> {
        let entry = self
            .entries
            .get(&handle.slot())
            .ok_or(AssetError::StaleHandle)?;
        if entry.id.kind() != T::KIND
            || handle.generation() != entry.current_generation
            || !entry
                .current
                .as_ref()
                .is_some_and(|payload| payload.as_any().type_id() == TypeId::of::<T>())
        {
            return Err(AssetError::StaleHandle);
        }
        Ok(entry.id.clone())
    }

    /// Load synchronously for compatibility adapters while using the typed store as owner.
    pub fn load_sync<T: Asset>(&mut self, path: &Path) -> Result<Handle<T>, AssetError> {
        let relative = self.compatibility_path(path)?;
        let id = AssetId::from_path(T::KIND, &self.config.project_root, &relative)?;
        if let Some(slot) = self.id_to_slot.get(&id).copied() {
            let entry = self.entries.get(&slot).ok_or(AssetError::StaleHandle)?;
            if entry.current.is_some() {
                let handle = Handle::from_parts(slot, entry.current_generation);
                self.get(handle)?;
                return Ok(handle);
            }
        }
        let asset = T::load(&self.resolve_path(&id))
            .map_err(|error| AssetError::Decode(error.to_string()))?;
        self.insert_ready(&relative, asset)
    }

    /// Preserve legacy synchronous loads for absolute files outside the project root.
    pub fn load_legacy_sync<T: Asset>(&mut self, path: &Path) -> Result<Handle<T>, AssetError> {
        self.ensure_accepting()?;
        if !path.is_absolute() || path.starts_with(&self.config.project_root) {
            return self.load_sync(path);
        }
        let id = AssetId::from_legacy_external_path(T::KIND, path)?;
        if let Some(slot) = self.id_to_slot.get(&id).copied() {
            let entry = self.entries.get(&slot).ok_or(AssetError::StaleHandle)?;
            if entry.current.is_some() {
                let handle = Handle::from_parts(slot, entry.current_generation);
                self.get(handle)?;
                return Ok(handle);
            }
        }
        let asset = T::load(path).map_err(|error| AssetError::Decode(error.to_string()))?;
        self.insert_ready_with_id(id, asset)
    }

    pub(super) fn insert_ready_with_id<T: Asset>(
        &mut self,
        id: AssetId,
        asset: T,
    ) -> Result<Handle<T>, AssetError> {
        self.ensure_accepting()?;
        if let Some(slot) = self.id_to_slot.get(&id).copied() {
            let entry = self.entries.get(&slot).ok_or(AssetError::StaleHandle)?;
            if entry.current.is_some() {
                if !entry
                    .current
                    .as_ref()
                    .is_some_and(|payload| payload.as_any().is::<T>())
                {
                    return Err(AssetError::PayloadTypeMismatch);
                }
                return Ok(Handle::from_parts(slot, entry.current_generation));
            }
        }
        let slot = if let Some(slot) = self.id_to_slot.get(&id).copied() {
            slot
        } else {
            let slot = self.allocate_slot()?;
            self.id_to_slot.insert(id.clone(), slot);
            self.entries.insert(slot, AssetEntry::new(id.clone()));
            slot
        };
        let pending_ticket = self
            .entries
            .get(&slot)
            .and_then(|entry| entry.pending.as_ref().map(|pending| pending.request.ticket));
        if let Some(ticket) = pending_ticket {
            if let Some(pending) = self
                .entries
                .get(&slot)
                .and_then(|entry| entry.pending.as_ref())
            {
                pending.request.cancel.cancel();
            }
            self.mark_ticket_cancelled(ticket);
        }
        let entry = self.entries.get_mut(&slot).ok_or(AssetError::StaleHandle)?;
        let generation = entry
            .requested_generation
            .max(entry.current_generation)
            .checked_add(1)
            .ok_or(AssetError::IdentifierExhausted)?;
        entry.pending = None;
        entry.current = Some(Arc::new(asset));
        entry.current_generation = generation;
        entry.requested_generation = generation;
        entry.state = LoadStateView::Ready { generation };
        entry.applied = None;
        Ok(Handle::from_parts(slot, generation))
    }

    /// Convert a legacy path into the store's project-relative path form.
    pub fn compatibility_path(&self, path: &Path) -> Result<PathBuf, AssetError> {
        let relative = if path.is_absolute() {
            path.strip_prefix(&self.config.project_root)
                .map_err(|_| AssetError::InvalidPath(super::super::AssetIdError::PathEscape))?
                .to_path_buf()
        } else {
            path.to_path_buf()
        };
        let normalized = super::canonicalize(&self.config.project_root, &relative)?;
        Ok(PathBuf::from(normalized.as_str()))
    }

    /// Check whether a handle currently resolves to a ready asset.
    pub fn contains_handle<T>(&self, handle: Handle<T>) -> bool {
        self.entries.get(&handle.slot()).is_some_and(|entry| {
            entry.current_generation == handle.generation() && entry.current.is_some()
        })
    }

    /// Return a fresh handle for the currently active generation of an identity.
    pub fn current_handle<T: Asset>(&self, asset: &AssetId) -> Result<Handle<T>, AssetError> {
        if asset.kind() != T::KIND {
            return Err(AssetError::StaleHandle);
        }
        let slot = self
            .id_to_slot
            .get(asset)
            .copied()
            .ok_or(AssetError::StaleHandle)?;
        let entry = self.entries.get(&slot).ok_or(AssetError::StaleHandle)?;
        if entry.current_generation == 0 {
            return Err(AssetError::NotReady);
        }
        if entry
            .current
            .as_ref()
            .is_some_and(|payload| payload.as_any().type_id() != TypeId::of::<T>())
        {
            return Err(AssetError::PayloadTypeMismatch);
        }
        Ok(Handle::from_parts(slot, entry.current_generation))
    }
}
