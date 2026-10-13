//! Preflighted synchronous insertion used by bounded host texture snapshots.
use super::{AssetError, AssetStore};
use crate::asset::{Asset, Handle};
use std::{any::TypeId, collections::HashSet, path::PathBuf};

impl AssetStore {
    pub(crate) fn insert_ready_batch<T: Asset>(
        &mut self,
        assets: Vec<(PathBuf, T)>,
    ) -> Result<Vec<Handle<T>>, AssetError> {
        self.ensure_accepting()?;
        let entries = assets
            .into_iter()
            .map(|(path, asset)| self.asset_id::<T>(&path).map(|id| (id, asset)))
            .collect::<Result<Vec<_>, _>>()?;
        let mut new_ids = HashSet::new();
        for (id, _) in &entries {
            if let Some(slot) = self.id_to_slot.get(id) {
                let entry = self.entries.get(slot).ok_or(AssetError::StaleHandle)?;
                if entry
                    .current
                    .as_ref()
                    .is_some_and(|payload| !payload.as_any().is::<T>())
                    || entry
                        .pending
                        .as_ref()
                        .is_some_and(|pending| pending.request.expected_type != TypeId::of::<T>())
                {
                    return Err(AssetError::PayloadTypeMismatch);
                }
                if entry.current.is_none() {
                    entry
                        .current_generation
                        .max(entry.requested_generation)
                        .checked_add(1)
                        .ok_or(AssetError::IdentifierExhausted)?;
                }
            } else {
                new_ids.insert(id.clone());
            }
        }
        let slots = u32::try_from(new_ids.len()).map_err(|_| AssetError::IdentifierExhausted)?;
        self.next_slot
            .checked_add(slots)
            .ok_or(AssetError::IdentifierExhausted)?;
        // All fallible identity/type/capacity checks precede the first mutation.
        entries
            .into_iter()
            .map(|(id, asset)| self.insert_ready_with_id(id, asset))
            .collect()
    }
}
