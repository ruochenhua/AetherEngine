//! Worker, ticket, and path helpers for [`super::AssetStore`].

use super::{AssetStore, DecodeFn, LoadJob};
use crate::asset::store_types::{
    AssetError, AssetPayload, AssetResult, CancellationToken, LoadRequest, LoadStateView,
    LoadTicket, PendingLoad, ReloadTicket, TicketRecord,
};
use crate::asset::{Asset, AssetId, Handle};
use std::any::TypeId;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

impl AssetStore {
    pub(super) fn start_load<T: Asset>(
        &mut self,
        id: AssetId,
        slot: u32,
        from_generation: u32,
        generation: u32,
    ) -> Result<(Handle<T>, LoadTicket, ReloadTicket), AssetError> {
        self.ensure_worker()?;
        let sequence = self.take_sequence()?;
        let ticket = self.take_ticket()?;
        let token = CancellationToken::new();
        let request = LoadRequest {
            asset: id.clone(),
            slot,
            generation,
            sequence,
            expected_type: TypeId::of::<T>(),
            cancel: token.clone(),
            ticket,
            from_generation,
        };
        let reload = ReloadTicket {
            asset: id.clone(),
            from_generation,
            to_generation: generation,
            sequence,
            load_ticket: ticket,
        };
        self.tickets.insert(
            ticket.id,
            TicketRecord {
                slot,
                generation,
                sequence,
                token,
                cancelled: false,
            },
        );
        let path = self.resolve_path(&id);
        let entry = self.entries.get_mut(&slot).ok_or(AssetError::StaleHandle)?;
        entry.requested_generation = generation;
        entry.last_ticket = Some(ticket);
        entry.applied = None;
        entry.state = LoadStateView::Loading { generation };
        entry.pending = Some(PendingLoad {
            request: request.clone(),
            reload: reload.clone(),
        });
        let decode: DecodeFn = Box::new(|path| {
            T::load(path)
                .map(|asset| Arc::new(asset) as Arc<dyn AssetPayload>)
                .map_err(|error| AssetError::Decode(error.to_string()))
        });
        let job = LoadJob {
            request,
            path,
            decode,
        };
        if self
            .job_tx
            .as_ref()
            .ok_or(AssetError::WorkerStopped)?
            .send(job)
            .is_err()
        {
            self.fail_pending(slot, AssetError::WorkerStopped);
            return Err(AssetError::WorkerStopped);
        }
        Ok((Handle::from_parts(slot, generation), ticket, reload))
    }

    pub(super) fn ensure_worker(&mut self) -> Result<(), AssetError> {
        if self.shutting_down || self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            return Err(AssetError::WorkerStopped);
        }
        if self.worker.is_some() {
            return Ok(());
        }
        let (job_tx, job_rx) = mpsc::channel::<LoadJob>();
        let (result_tx, result_rx) = mpsc::channel::<AssetResult>();
        let worker = thread::Builder::new()
            .name("aether-asset-worker".into())
            .spawn(move || {
                while let Ok(job) = job_rx.recv() {
                    let result = if job.request.cancel.is_cancelled() {
                        Err(AssetError::Cancelled)
                    } else {
                        (job.decode)(&job.path)
                    };
                    let result = if job.request.cancel.is_cancelled() {
                        Err(AssetError::Cancelled)
                    } else {
                        result
                    };
                    let message = match result {
                        Ok(payload) => AssetResult::Ready {
                            request: job.request,
                            payload,
                        },
                        Err(error) => AssetResult::Failed {
                            request: job.request,
                            error,
                        },
                    };
                    if result_tx.send(message).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| {
                AssetError::Decode(format!("could not start asset worker: {error}"))
            })?;
        self.job_tx = Some(job_tx);
        self.result_rx = Some(result_rx);
        self.worker = Some(worker);
        Ok(())
    }

    pub(super) fn ensure_accepting(&self) -> Result<(), AssetError> {
        if self.shutting_down {
            Err(AssetError::WorkerStopped)
        } else {
            Ok(())
        }
    }

    pub(super) fn allocate_slot(&mut self) -> Result<u32, AssetError> {
        let slot = self.next_slot;
        self.next_slot = self
            .next_slot
            .checked_add(1)
            .ok_or(AssetError::IdentifierExhausted)?;
        Ok(slot)
    }

    fn take_ticket(&mut self) -> Result<LoadTicket, AssetError> {
        let id = self.next_ticket;
        self.next_ticket = self
            .next_ticket
            .checked_add(1)
            .ok_or(AssetError::IdentifierExhausted)?;
        Ok(LoadTicket { id })
    }

    fn take_sequence(&mut self) -> Result<u64, AssetError> {
        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(AssetError::IdentifierExhausted)?;
        Ok(sequence)
    }

    pub(super) fn completed_ticket(
        &mut self,
        slot: u32,
        generation: u32,
    ) -> Result<LoadTicket, AssetError> {
        let ticket = self.take_ticket()?;
        self.tickets.insert(
            ticket.id,
            TicketRecord {
                slot,
                generation,
                sequence: 0,
                token: CancellationToken::new(),
                cancelled: false,
            },
        );
        if let Some(entry) = self.entries.get_mut(&slot) {
            entry.last_ticket = Some(ticket);
        }
        Ok(ticket)
    }

    pub(super) fn validate_request(&self, request: &LoadRequest) -> Result<(), AssetError> {
        let record = self
            .tickets
            .get(&request.ticket.id)
            .ok_or(AssetError::StaleReloadTicket)?;
        if record.cancelled
            || record.slot != request.slot
            || record.generation != request.generation
            || record.sequence != request.sequence
        {
            return Err(if record.cancelled {
                AssetError::Cancelled
            } else {
                AssetError::StaleReloadTicket
            });
        }
        Ok(())
    }

    pub(super) fn mark_ticket_cancelled(&mut self, ticket: LoadTicket) {
        if let Some(record) = self.tickets.get_mut(&ticket.id) {
            record.cancelled = true;
            record.token.cancel();
        }
    }

    pub(super) fn resolve_path(&self, id: &AssetId) -> PathBuf {
        self.config.project_root.join(id.path().as_str())
    }

    fn fail_pending(&mut self, slot: u32, error: AssetError) {
        if let Some(entry) = self.entries.get_mut(&slot) {
            let generation = entry.requested_generation;
            entry.pending = None;
            entry.state = LoadStateView::Failed {
                generation,
                error,
                has_last_good: entry.current.is_some(),
            };
        }
    }
}
