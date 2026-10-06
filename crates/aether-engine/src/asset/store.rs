//! Typed, CPU-side asset lifecycle with a single decoding worker.

use super::Handle;
use std::any::{Any, TypeId};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

mod types;
pub use types::*;

type JobFn = Box<dyn FnOnce() -> Result<Arc<dyn AssetPayload>, AssetError> + Send>;

struct Job {
    slot: u64,
    generation: u32,
    sequence: u64,
    ticket: u128,
    kind: AssetKind,
    cancelled: types::CancellationToken,
    run: JobFn,
}

struct TicketRecord {
    slot: u64,
    sequence: u64,
    cancelled: types::CancellationToken,
}

struct Entry {
    id: AssetId,
    generation: u32,
    sequence: u64,
    ticket: u128,
    state: LoadStateView,
    last_good: Option<Arc<dyn Any + Send + Sync>>,
    type_id: TypeId,
}

/// Single-worker typed CPU asset store. GPU uploads remain owned by renderer code.
pub struct AssetStore {
    config: AssetStoreConfig,
    next_slot: u64,
    next_ticket: u128,
    next_sequence: u64,
    entries: HashMap<u64, Entry>,
    ids: HashMap<AssetId, u64>,
    tickets: HashMap<u128, TicketRecord>,
    cancelled: HashSet<u128>,
    jobs: Option<Sender<Job>>,
    results: Receiver<AssetResult>,
    worker: Option<JoinHandle<()>>,
    stopped: bool,
    last_frame_id: u64,
}

impl AssetStore {
    /// Start the asset worker.
    pub fn new(config: AssetStoreConfig) -> Self {
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (result_tx, result_rx) = mpsc::channel::<AssetResult>();
        let worker = thread::spawn(move || {
            while let Ok(job) = job_rx.recv() {
                let result = if job.cancelled.is_cancelled() {
                    Err(AssetError::Cancelled)
                } else {
                    (job.run)()
                };
                if result_tx
                    .send(AssetResult {
                        slot: job.slot,
                        generation: job.generation,
                        sequence: job.sequence,
                        ticket: job.ticket,
                        kind: job.kind,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            config,
            next_slot: 1,
            next_ticket: 1,
            next_sequence: 1,
            entries: HashMap::new(),
            ids: HashMap::new(),
            tickets: HashMap::new(),
            cancelled: HashSet::new(),
            jobs: Some(job_tx),
            results: result_rx,
            worker: Some(worker),
            stopped: false,
            last_frame_id: 0,
        }
    }

    /// Request a typed asset; repeated kind/path requests share one slot and in-flight decode.
    pub fn request<T: types::TypedAsset>(
        &mut self,
        path: &Path,
    ) -> Result<(Handle<T>, LoadTicket), AssetError> {
        if self.stopped {
            return Err(AssetError::WorkerStopped);
        }
        let id = AssetId {
            kind: T::KIND,
            path: types::canonicalize(&self.config.project_root, path)?,
        };
        let slot = if let Some(slot) = self.ids.get(&id).copied() {
            slot
        } else {
            let slot = self.next_slot;
            self.next_slot += 1;
            self.ids.insert(id.clone(), slot);
            self.entries.insert(
                slot,
                Entry {
                    id: id.clone(),
                    generation: 1,
                    sequence: 0,
                    ticket: 0,
                    state: LoadStateView::Loading { generation: 1 },
                    last_good: None,
                    type_id: TypeId::of::<T>(),
                },
            );
            slot
        };
        let (sequence, type_id, generation, existing_ticket) = self
            .entries
            .get(&slot)
            .map(|entry| {
                (
                    entry.sequence,
                    entry.type_id,
                    entry.generation,
                    entry.ticket,
                )
            })
            .ok_or(AssetError::StaleHandle)?;
        if type_id != TypeId::of::<T>() {
            return Err(AssetError::PayloadTypeMismatch);
        }
        let ticket = if sequence == 0 {
            let sequence = self.next_sequence;
            let ticket = self.new_ticket(slot, sequence);
            if let Some(entry) = self.entries.get_mut(&slot) {
                entry.ticket = ticket.0;
            }
            self.enqueue::<T>(slot, generation, ticket.0, id)?;
            ticket
        } else {
            LoadTicket(existing_ticket)
        };
        Ok((Handle::with_generation(slot, generation), ticket))
    }

    /// Read lifecycle state, rejecting released or stale handles.
    pub fn state<T>(&self, handle: Handle<T>) -> Result<LoadStateView, AssetError> {
        let entry = self.entry(handle)?;
        Ok(entry.state.clone())
    }

    /// Get the last successfully applied payload, including during failed reload.
    pub fn get<T: types::TypedAsset>(&self, handle: Handle<T>) -> Result<Arc<T>, AssetError> {
        let entry = self.entry(handle)?;
        let payload = entry.last_good.clone().ok_or_else(|| match &entry.state {
            LoadStateView::Failed { error, .. } => error.clone(),
            _ => AssetError::Decode("asset is not ready".into()),
        })?;
        Arc::downcast(payload).map_err(|_| AssetError::PayloadTypeMismatch)
    }

    /// Queue a reload while retaining the currently available payload.
    pub fn reload<T: types::TypedAsset>(
        &mut self,
        handle: Handle<T>,
    ) -> Result<ReloadTicket, AssetError> {
        let slot = handle.id();
        let (id, from) = {
            let entry = self.entry(handle)?;
            (entry.id.clone(), entry.generation)
        };
        let sequence = self.next_sequence;
        let ticket = self.new_ticket(slot, sequence);
        if let Some(entry) = self.entries.get_mut(&slot) {
            entry.sequence = sequence;
            entry.ticket = ticket.0;
            entry.state = LoadStateView::Loading { generation: from };
        }
        self.enqueue::<T>(slot, from, ticket.0, id.clone())?;
        Ok(ReloadTicket {
            asset: id,
            from_generation: from,
            to_generation: from + 1,
            sequence,
        })
    }

    /// Drain at most `max` completed worker results.
    pub fn poll_results(&mut self, max: usize) -> Result<Vec<AssetResult>, AssetError> {
        let mut out = Vec::with_capacity(max);
        for _ in 0..max {
            match self.results.try_recv() {
                Ok(result) => out.push(result),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return Err(AssetError::WorkerStopped),
            }
        }
        Ok(out)
    }

    /// Apply a decoded result at a monotonically increasing main-thread frame boundary.
    pub fn apply_result(
        &mut self,
        result: AssetResult,
        boundary: FrameBoundary,
    ) -> Result<ReloadTicket, AssetError> {
        if boundary.frame_id == 0 || boundary.frame_id < self.last_frame_id {
            return Err(AssetError::InvalidFrameBoundary);
        }
        self.last_frame_id = boundary.frame_id;
        let ticket = self
            .tickets
            .get(&result.ticket)
            .ok_or(AssetError::StaleReloadTicket)?;
        if ticket.slot != result.slot || ticket.sequence != result.sequence {
            return Err(AssetError::StaleReloadTicket);
        }
        if self.cancelled.contains(&result.ticket) {
            if let Some(entry) = self.entries.get_mut(&result.slot) {
                entry.state = LoadStateView::Failed {
                    generation: entry.generation,
                    error: AssetError::Cancelled,
                    has_last_good: entry.last_good.is_some(),
                };
            }
            return Err(AssetError::Cancelled);
        }
        let entry = self
            .entries
            .get_mut(&result.slot)
            .ok_or(AssetError::StaleHandle)?;
        if entry.generation != result.generation || entry.sequence != result.sequence {
            return Err(AssetError::StaleReloadTicket);
        }
        if entry.id.kind != result.kind {
            return Err(AssetError::PayloadKindMismatch);
        }
        let from = entry.generation;
        let outcome = match result.result {
            Ok(payload) => {
                if payload.kind() != entry.id.kind {
                    return Err(AssetError::PayloadKindMismatch);
                }
                let value = payload.into_any();
                if value.as_ref().type_id() != entry.type_id {
                    return Err(AssetError::PayloadTypeMismatch);
                }
                let had_last_good = entry.last_good.is_some();
                entry.last_good = Some(value);
                entry.generation = result.generation + u32::from(had_last_good);
                entry.state = LoadStateView::Ready {
                    generation: entry.generation,
                };
                Ok(())
            }
            Err(error) => {
                entry.state = LoadStateView::Failed {
                    generation: entry.generation,
                    error: error.clone(),
                    has_last_good: entry.last_good.is_some(),
                };
                Err(error)
            }
        };
        outcome?;
        Ok(ReloadTicket {
            asset: entry.id.clone(),
            from_generation: from,
            to_generation: entry.generation,
            sequence: result.sequence,
        })
    }

    /// Cancel a request capability. Repeated cancellation is safe.
    pub fn cancel(&mut self, ticket: &LoadTicket) -> Result<(), AssetError> {
        if let Some(record) = self.tickets.get(&ticket.0) {
            record.cancelled.cancel();
            self.cancelled.insert(ticket.0);
            Ok(())
        } else {
            Err(AssetError::StaleHandle)
        }
    }

    /// Release a slot and invalidate all handles referring to it.
    pub fn release<T>(&mut self, handle: Handle<T>) -> Result<(), AssetError> {
        let slot = handle.id();
        let entry = self.entry(handle)?;
        let id = entry.id.clone();
        self.entries.remove(&slot);
        self.ids.remove(&id);
        Ok(())
    }

    /// Stop accepting work and wait only up to the configured timeout.
    pub fn shutdown(&mut self) -> Result<(), AssetError> {
        self.stopped = true;
        for entry in self.entries.values_mut() {
            if matches!(entry.state, LoadStateView::Loading { .. }) {
                if let Some(ticket) = self.tickets.get(&entry.ticket) {
                    ticket.cancelled.cancel();
                    self.cancelled.insert(entry.ticket);
                }
                entry.state = LoadStateView::Failed {
                    generation: entry.generation,
                    error: AssetError::Cancelled,
                    has_last_good: entry.last_good.is_some(),
                };
            }
        }
        self.jobs.take();
        let deadline = Instant::now() + self.config.join_timeout;
        loop {
            while self.results.try_recv().is_ok() {}
            let finished = self.worker.as_ref().map_or(true, JoinHandle::is_finished);
            if finished {
                if let Some(worker) = self.worker.take() {
                    let _ = worker.join();
                }
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(AssetError::ShutdownTimeout);
            }
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn entry<T>(&self, handle: Handle<T>) -> Result<&Entry, AssetError> {
        self.entries
            .get(&handle.id())
            .filter(|entry| entry.generation == handle.generation())
            .ok_or(AssetError::StaleHandle)
    }

    fn new_ticket(&mut self, slot: u64, sequence: u64) -> LoadTicket {
        let ticket = LoadTicket(self.next_ticket);
        self.next_ticket += 1;
        self.tickets.insert(
            ticket.0,
            TicketRecord {
                slot,
                sequence,
                cancelled: types::CancellationToken::new(),
            },
        );
        ticket
    }

    fn enqueue<T: types::TypedAsset>(
        &mut self,
        slot: u64,
        generation: u32,
        ticket: u128,
        id: AssetId,
    ) -> Result<(), AssetError> {
        let path = self.config.project_root.join(id.path.as_str());
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        if let Some(entry) = self.entries.get_mut(&slot) {
            entry.sequence = sequence;
        }
        let cancelled = self
            .tickets
            .get(&ticket)
            .ok_or(AssetError::StaleHandle)?
            .cancelled
            .clone();
        let decoder_cancellation = cancelled.clone();
        let job = Job {
            slot,
            generation,
            sequence,
            ticket,
            kind: T::KIND,
            cancelled,
            run: Box::new(move || {
                T::load_cancellable(&path, &decoder_cancellation)
                    .map(|value| Arc::new(value) as Arc<dyn AssetPayload>)
            }),
        };
        self.jobs
            .as_ref()
            .ok_or(AssetError::WorkerStopped)?
            .send(job)
            .map_err(|_| AssetError::WorkerStopped)
    }
}

impl Drop for AssetStore {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}
