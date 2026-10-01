//! Typed CPU asset ownership, asynchronous loading, and frame-boundary commits.

mod gpu;
mod handles;
mod lifecycle;
mod worker;

#[cfg(test)]
#[path = "store/frame_boundary_tests.rs"]
mod frame_boundary_tests;
#[cfg(test)]
#[path = "store/hot_reload_acceptance.rs"]
mod hot_reload_acceptance;
#[cfg(test)]
mod store_tests;

use super::store_types::{AssetEntry, TicketRecord};
use super::{Asset, AssetId, Handle};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;

pub use super::store_types::{
    ApplyOutcome, AssetError, AssetPayload, AssetResult, AssetStoreConfig, CancellationToken,
    FrameBoundary, GpuAssetKey, GpuCommitCache, GpuCommitContext, LoadRequest, LoadStateView,
    LoadTicket, ReloadTicket,
};

type DecodeFn = Box<dyn FnOnce(&Path) -> Result<Arc<dyn AssetPayload>, AssetError> + Send>;

struct LoadJob {
    request: LoadRequest,
    path: PathBuf,
    decode: DecodeFn,
}

/// The single typed facade for CPU asset loading and GPU generation commits.
pub struct AssetStore {
    config: AssetStoreConfig,
    entries: HashMap<u32, AssetEntry>,
    id_to_slot: HashMap<AssetId, u32>,
    tickets: HashMap<u128, TicketRecord>,
    next_slot: u32,
    next_ticket: u128,
    next_sequence: u64,
    job_tx: Option<Sender<LoadJob>>,
    result_rx: Option<Receiver<AssetResult>>,
    worker: Option<JoinHandle<()>>,
    deferred_results: VecDeque<AssetResult>,
    shutting_down: bool,
    last_applied_boundary: Option<u64>,
    last_gpu_boundary: Option<u64>,
}

impl AssetStore {
    /// Create a store and start its worker lazily on the first async request.
    pub fn new(mut config: AssetStoreConfig) -> Self {
        if !config.project_root.is_absolute() {
            let root = std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(&config.project_root);
            config.project_root = root;
        }
        Self {
            config,
            entries: HashMap::new(),
            id_to_slot: HashMap::new(),
            tickets: HashMap::new(),
            next_slot: 1,
            next_ticket: 1,
            next_sequence: 1,
            job_tx: None,
            result_rx: None,
            worker: None,
            deferred_results: VecDeque::new(),
            shutting_down: false,
            last_applied_boundary: None,
            last_gpu_boundary: None,
        }
    }

    /// Queue a typed CPU decode, deduplicating an existing identity or in-flight request.
    pub fn request<T: Asset>(
        &mut self,
        path: &Path,
    ) -> Result<(Handle<T>, LoadTicket), AssetError> {
        self.ensure_accepting()?;
        let id = AssetId::from_path(T::KIND, &self.config.project_root, path)?;
        if let Some(slot) = self.id_to_slot.get(&id).copied() {
            let (pending, current, last_ticket, current_generation) = {
                let entry = self.entries.get(&slot).ok_or(AssetError::StaleHandle)?;
                if entry
                    .current
                    .as_ref()
                    .is_some_and(|payload| !payload.as_any().is::<T>())
                    || entry.pending.as_ref().is_some_and(|pending| {
                        pending.request.expected_type != std::any::TypeId::of::<T>()
                    })
                {
                    return Err(AssetError::PayloadTypeMismatch);
                }
                (
                    entry
                        .pending
                        .as_ref()
                        .map(|pending| (pending.request.generation, pending.request.ticket)),
                    entry.current.is_some(),
                    entry.last_ticket,
                    entry.current_generation,
                )
            };
            if let Some((generation, ticket)) = pending {
                return Ok((Handle::from_parts(slot, generation), ticket));
            }
            if current {
                let ticket = match last_ticket {
                    Some(ticket) => ticket,
                    None => self.completed_ticket(slot, current_generation)?,
                };
                return Ok((Handle::from_parts(slot, current_generation), ticket));
            }
        } else {
            self.ensure_worker()?;
        }

        let (slot, from_generation, generation) = if let Some(slot) = self.id_to_slot.get(&id) {
            let entry = self.entries.get(slot).ok_or(AssetError::StaleHandle)?;
            let from = entry.current_generation;
            let target = entry
                .requested_generation
                .max(from)
                .checked_add(1)
                .ok_or(AssetError::IdentifierExhausted)?;
            (*slot, from, target)
        } else {
            let slot = self.allocate_slot()?;
            self.id_to_slot.insert(id.clone(), slot);
            self.entries.insert(slot, AssetEntry::new(id.clone()));
            (slot, 0, 1)
        };
        let (handle, ticket, _) = self.start_load::<T>(id, slot, from_generation, generation)?;
        Ok((handle, ticket))
    }

    /// Insert an already decoded asset for synchronous compatibility adapters.
    pub fn insert_ready<T: Asset>(
        &mut self,
        path: &Path,
        asset: T,
    ) -> Result<Handle<T>, AssetError> {
        let id = AssetId::from_path(T::KIND, &self.config.project_root, path)?;
        self.insert_ready_with_id(id, asset)
    }

    /// Read the latest state for a valid handle generation.
    pub fn state<T: Asset>(&self, handle: Handle<T>) -> Result<LoadStateView, AssetError> {
        let entry = self
            .entries
            .get(&handle.slot())
            .ok_or(AssetError::StaleHandle)?;
        if entry.id.kind() != T::KIND {
            return Err(AssetError::StaleHandle);
        }
        if entry
            .current
            .as_ref()
            .is_some_and(|payload| !payload.as_any().is::<T>())
        {
            return Err(AssetError::PayloadTypeMismatch);
        }
        let generation_is_known = handle.generation() == entry.current_generation
            || handle.generation() == entry.requested_generation;
        if !generation_is_known {
            return Err(AssetError::StaleHandle);
        }
        Ok(entry.state.clone())
    }

    /// Resolve the current CPU payload for a handle, preserving a last-known-good generation.
    pub fn get<T: Asset>(&self, handle: Handle<T>) -> Result<Arc<T>, AssetError> {
        let entry = self
            .entries
            .get(&handle.slot())
            .ok_or(AssetError::StaleHandle)?;
        if entry.id.kind() != T::KIND {
            return Err(AssetError::StaleHandle);
        }
        if handle.generation() != entry.current_generation {
            if handle.generation() == entry.requested_generation {
                return Err(match &entry.state {
                    LoadStateView::Failed { error, .. } => error.clone(),
                    _ => AssetError::NotReady,
                });
            }
            return Err(AssetError::StaleHandle);
        }
        let payload = entry.current.clone().ok_or_else(|| match &entry.state {
            LoadStateView::Failed { error, .. } => error.clone(),
            _ => AssetError::NotReady,
        })?;
        Arc::downcast::<T>(payload.into_any()).map_err(|_| AssetError::PayloadTypeMismatch)
    }

    /// Queue a reload and return a versioned commit ticket.
    pub fn reload<T: Asset>(&mut self, handle: Handle<T>) -> Result<ReloadTicket, AssetError> {
        self.ensure_accepting()?;
        self.ensure_worker()?;
        let slot = handle.slot();
        let (id, from_generation, generation, pending_ticket) = {
            let entry = self.entries.get_mut(&slot).ok_or(AssetError::StaleHandle)?;
            if entry.id.kind() != T::KIND || handle.generation() != entry.current_generation {
                return Err(AssetError::StaleHandle);
            }
            if entry.current.is_none() {
                return Err(AssetError::NotReady);
            }
            let pending_ticket = if let Some(pending) = entry.pending.take() {
                pending.request.cancel.cancel();
                Some(pending.request.ticket)
            } else {
                None
            };
            let from = entry.current_generation;
            let target = entry
                .requested_generation
                .max(from)
                .checked_add(1)
                .ok_or(AssetError::IdentifierExhausted)?;
            (entry.id.clone(), from, target, pending_ticket)
        };
        if let Some(ticket) = pending_ticket {
            self.mark_ticket_cancelled(ticket);
        }
        let (_, _, ticket) = self.start_load::<T>(id, slot, from_generation, generation)?;
        Ok(ticket)
    }

    /// Recover the caller-owned cancellation capability for a reload request.
    pub fn load_ticket(&self, ticket: &ReloadTicket) -> Result<LoadTicket, AssetError> {
        let record = self
            .tickets
            .get(&ticket.load_ticket.id)
            .ok_or(AssetError::StaleReloadTicket)?;
        if record.sequence != ticket.sequence || record.generation != ticket.to_generation {
            return Err(AssetError::StaleReloadTicket);
        }
        Ok(ticket.load_ticket)
    }

    /// Drain up to `max` worker results without blocking the app thread.
    pub fn poll_results(&mut self, max: usize) -> Result<Vec<AssetResult>, AssetError> {
        if self.shutting_down {
            return Err(AssetError::WorkerStopped);
        }
        let Some(receiver) = self.result_rx.as_ref() else {
            let count = max.min(self.deferred_results.len());
            return Ok(self.deferred_results.drain(..count).collect());
        };
        let mut results = Vec::with_capacity(max.min(32));
        while results.len() < max {
            let Some(result) = self.deferred_results.pop_front() else {
                break;
            };
            results.push(result);
        }
        while results.len() < max {
            match receiver.try_recv() {
                Ok(result) => results.push(result),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        Ok(results)
    }

    /// Return a result to the front of the queue when another adapter owns it.
    pub fn defer_result(&mut self, result: AssetResult) {
        self.deferred_results.push_front(result);
    }

    /// Apply one worker result on the app thread at a monotonic frame boundary.
    pub fn apply_result(
        &mut self,
        result: AssetResult,
        boundary: FrameBoundary,
    ) -> Result<ApplyOutcome, AssetError> {
        if self
            .last_applied_boundary
            .is_some_and(|last| boundary.frame_id < last)
        {
            self.deferred_results.push_front(result);
            return Err(AssetError::InvalidFrameBoundary);
        }
        let request = result.request().clone();
        self.validate_request(&request)?;
        if request.cancel.is_cancelled() {
            self.cancel(&request.ticket)?;
            return Err(AssetError::Cancelled);
        }
        let entry = self
            .entries
            .get_mut(&request.slot)
            .ok_or(AssetError::StaleHandle)?;
        let pending = entry
            .pending
            .as_ref()
            .ok_or(AssetError::StaleReloadTicket)?;
        if pending.request.sequence != request.sequence
            || pending.request.generation != request.generation
            || pending.request.from_generation != request.from_generation
            || pending.request.asset != request.asset
            || pending.request.ticket != request.ticket
        {
            return Err(AssetError::StaleReloadTicket);
        }
        let reload = pending.reload.clone();
        let state = match result {
            AssetResult::Ready { payload, .. }
                if payload.kind() == request.asset.kind()
                    && payload.as_any().type_id() == request.expected_type =>
            {
                if let Some(old) = entry.current.take() {
                    entry.retired.push((entry.current_generation, old));
                }
                entry.current = Some(payload);
                entry.current_generation = request.generation;
                entry.state = LoadStateView::Ready {
                    generation: request.generation,
                };
                entry.applied = Some(reload.clone());
                entry.pending = None;
                entry.state.clone()
            }
            AssetResult::Ready { payload, .. } => {
                let error = if payload.kind() != request.asset.kind() {
                    AssetError::PayloadKindMismatch
                } else {
                    AssetError::PayloadTypeMismatch
                };
                apply_failure(entry, request.generation, error.clone());
                entry.pending = None;
                self.last_applied_boundary = Some(boundary.frame_id);
                return Err(error);
            }
            AssetResult::Failed { error, .. } => {
                apply_failure(entry, request.generation, error.clone());
                entry.pending = None;
                self.last_applied_boundary = Some(boundary.frame_id);
                return Ok(ApplyOutcome {
                    asset: request.asset,
                    ticket: reload,
                    state: entry.state.clone(),
                    payload_kind: entry.current.as_ref().map(|payload| payload.kind()),
                });
            }
        };
        entry.last_ticket = Some(request.ticket);
        self.last_applied_boundary = Some(boundary.frame_id);
        Ok(ApplyOutcome {
            asset: request.asset,
            ticket: reload,
            state,
            payload_kind: entry.current.as_ref().map(|payload| payload.kind()),
        })
    }

    /// Cancel a queued or running decode; repeated cancellation is idempotent.
    pub fn cancel(&mut self, ticket: &LoadTicket) -> Result<(), AssetError> {
        let record = self
            .tickets
            .get_mut(&ticket.id)
            .ok_or(AssetError::StaleHandle)?;
        if record.cancelled {
            return Ok(());
        }
        record.cancelled = true;
        record.token.cancel();
        let slot = record.slot;
        let entry = self.entries.get_mut(&slot).ok_or(AssetError::StaleHandle)?;
        if entry
            .pending
            .as_ref()
            .is_some_and(|pending| pending.request.ticket == *ticket)
        {
            let pending = entry.pending.take().ok_or(AssetError::StaleReloadTicket)?;
            entry.state = LoadStateView::Failed {
                generation: pending.request.generation,
                error: AssetError::Cancelled,
                has_last_good: entry.current.is_some(),
            };
        }
        Ok(())
    }
}

/// Normalize a project-relative asset path and reject root escapes.
pub fn canonicalize(project_root: &Path, input: &Path) -> Result<super::CanonicalPath, AssetError> {
    super::CanonicalPath::new(project_root, input).map_err(AssetError::from)
}

fn apply_failure(entry: &mut AssetEntry, generation: u32, error: AssetError) {
    entry.state = LoadStateView::Failed {
        generation,
        error,
        has_last_good: entry.current.is_some(),
    };
    entry.pending = None;
}
