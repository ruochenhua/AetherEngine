//! App-thread GPU generation commit logic for [`super::AssetStore`].

use super::{AssetError, AssetStore, GpuAssetKey, GpuCommitContext, ReloadTicket};

impl AssetStore {
    /// Commit one applied generation into the GPU cache at a frame boundary.
    pub fn commit_gpu(
        &mut self,
        ticket: ReloadTicket,
        context: &mut GpuCommitContext<'_>,
    ) -> Result<(), AssetError> {
        if self
            .last_gpu_boundary
            .is_some_and(|last| context.boundary.frame_id < last)
        {
            return Err(AssetError::InvalidFrameBoundary);
        }
        let slot = self
            .id_to_slot
            .get(&ticket.asset)
            .copied()
            .ok_or(AssetError::StaleReloadTicket)?;
        let entry = self
            .entries
            .get_mut(&slot)
            .ok_or(AssetError::StaleReloadTicket)?;
        if entry.committed_sequences.contains(&ticket.sequence) {
            return Ok(());
        }
        if entry.current_generation != ticket.to_generation
            || entry.applied.as_ref() != Some(&ticket)
        {
            return Err(AssetError::StaleReloadTicket);
        }
        let payload = entry.current.clone().ok_or(AssetError::NotReady)?;
        context.cache.commit_asset(
            GpuAssetKey {
                asset: ticket.asset.clone(),
                generation: ticket.to_generation,
            },
            payload,
            context.boundary,
        )?;
        context.cache.collect_retired();
        entry.retired.clear();
        entry.committed_sequences.push(ticket.sequence);
        self.last_gpu_boundary = Some(context.boundary.frame_id);
        Ok(())
    }
}
