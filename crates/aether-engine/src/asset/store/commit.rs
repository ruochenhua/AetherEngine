//! Frame-boundary acceptance and publication transactions.

use super::*;
use std::any::Any;

impl AssetStore {
    /// Apply a decoded result at a monotonically increasing main-thread frame boundary.
    pub fn apply_result(
        &mut self,
        result: AssetResult,
        boundary: FrameBoundary,
    ) -> Result<ReloadTicket, AssetError> {
        if boundary.frame_id == 0 || boundary.frame_id < self.last_frame_id {
            return Err(AssetError::InvalidFrameBoundary);
        }
        let ticket = self
            .tickets
            .get(&result.ticket)
            .ok_or(AssetError::StaleReloadTicket)?;
        if ticket.slot != result.slot || ticket.sequence != result.sequence {
            return Err(AssetError::StaleReloadTicket);
        }
        let entry = self
            .entries
            .get_mut(&result.slot)
            .ok_or(AssetError::StaleHandle)?;
        if entry.generation != result.generation || entry.sequence != result.sequence {
            return Err(AssetError::StaleReloadTicket);
        }
        if self.cancelled.contains(&result.ticket) {
            entry.state = LoadStateView::Failed {
                generation: entry.generation,
                error: AssetError::Cancelled,
                has_last_good: entry.last_good.is_some(),
            };
            return Err(AssetError::Cancelled);
        }
        if entry.id.kind != result.kind {
            return Err(AssetError::PayloadKindMismatch);
        }
        self.last_frame_id = boundary.frame_id;
        let from = if entry.last_good.is_some() {
            entry.generation
        } else {
            0
        };
        match result.result {
            Ok(payload) => {
                if payload.kind() != entry.id.kind {
                    return Err(AssetError::PayloadKindMismatch);
                }
                let value = payload.into_any();
                if value.as_ref().type_id() != entry.type_id {
                    return Err(AssetError::PayloadTypeMismatch);
                }
                entry.pending = Some(PendingAsset {
                    ticket: result.ticket,
                    payload: value,
                });
            }
            Err(error) => {
                entry.state = LoadStateView::Failed {
                    generation: entry.generation,
                    error: error.clone(),
                    has_last_good: entry.last_good.is_some(),
                };
                return Err(error);
            }
        }
        Ok(ReloadTicket {
            asset: entry.id.clone(),
            from_generation: from,
            to_generation: from + 1,
            sequence: result.sequence,
            slot: result.slot,
            capability: result.ticket,
        })
    }

    /// Commit an accepted decode at a frame boundary. The callback performs any
    /// renderer work before the new generation becomes visible to readers.
    pub fn commit_result(
        &mut self,
        ticket: &ReloadTicket,
        boundary: FrameBoundary,
        commit: impl FnOnce(&AssetId, u32, &(dyn Any + Send + Sync)) -> Result<(), AssetError>,
    ) -> Result<(), AssetError> {
        if thread::current().id() != self.owner_thread {
            return Err(AssetError::WrongCommitThread);
        }
        if boundary.frame_id == 0 || boundary.frame_id < self.last_frame_id {
            return Err(AssetError::InvalidFrameBoundary);
        }
        self.last_frame_id = boundary.frame_id;
        let entry = self
            .entries
            .get(&ticket.slot)
            .ok_or(AssetError::StaleHandle)?;
        if entry.committed_ticket == Some(ticket.capability)
            && entry.generation == ticket.to_generation
            && entry.sequence == ticket.sequence
            && entry.id == ticket.asset
        {
            return Ok(());
        }
        let pending = entry
            .pending
            .as_ref()
            .ok_or(AssetError::StaleReloadTicket)?;
        if pending.ticket != ticket.capability || entry.sequence != ticket.sequence {
            return Err(AssetError::StaleReloadTicket);
        }
        if self.cancelled.contains(&ticket.capability) {
            let entry = self
                .entries
                .get_mut(&ticket.slot)
                .ok_or(AssetError::StaleHandle)?;
            entry.pending = None;
            entry.state = LoadStateView::Failed {
                generation: entry.generation,
                error: AssetError::Cancelled,
                has_last_good: entry.last_good.is_some(),
            };
            return Err(AssetError::Cancelled);
        }
        commit(&entry.id, ticket.to_generation, pending.payload.as_ref())?;
        let entry = self
            .entries
            .get_mut(&ticket.slot)
            .ok_or(AssetError::StaleHandle)?;
        let pending = entry.pending.take().ok_or(AssetError::StaleReloadTicket)?;
        entry.last_good = Some(pending.payload);
        entry.generation = ticket.to_generation;
        entry.committed_ticket = Some(ticket.capability);
        entry.state = LoadStateView::Ready {
            generation: entry.generation,
        };
        Ok(())
    }
}
