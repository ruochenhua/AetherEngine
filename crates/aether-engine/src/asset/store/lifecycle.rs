//! Bounded worker shutdown and drop behavior for [`super::AssetStore`].

use super::{AssetError, AssetStore, LoadStateView};
use std::thread;
use std::time::{Duration, Instant};

impl AssetStore {
    /// Stop accepting jobs and wait up to the configured deadline for the worker.
    pub fn shutdown(&mut self) -> Result<(), AssetError> {
        self.shutting_down = true;
        let pending: Vec<_> = self
            .entries
            .values_mut()
            .filter_map(|entry| {
                entry.pending.take().map(|pending| {
                    pending.request.cancel.cancel();
                    entry.state = LoadStateView::Failed {
                        generation: pending.request.generation,
                        error: AssetError::Cancelled,
                        has_last_good: entry.current.is_some(),
                    };
                    pending.request.ticket
                })
            })
            .collect();
        for ticket in pending {
            self.mark_ticket_cancelled(ticket);
        }
        self.job_tx.take();
        let Some(worker) = self.worker.as_ref() else {
            return Ok(());
        };
        let deadline =
            Instant::now() + Duration::from_millis(u64::from(self.config.join_timeout_ms));
        while !worker.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        if !worker.is_finished() {
            return Err(AssetError::ShutdownTimeout);
        }
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| AssetError::WorkerStopped)?;
        }
        if let Some(receiver) = &self.result_rx {
            while receiver.try_recv().is_ok() {}
        }
        self.deferred_results.clear();
        Ok(())
    }
}

impl Drop for AssetStore {
    fn drop(&mut self) {
        for entry in self.entries.values_mut() {
            if let Some(pending) = entry.pending.take() {
                pending.request.cancel.cancel();
            }
        }
        self.job_tx.take();
        // Dropping a JoinHandle detaches it; closing the job channel lets the worker exit.
    }
}
