//! Owned metadata for observing CPU store ownership without touching its worker.

use super::{AssetError, AssetId, AssetResult, AssetStore, LoadRequest, LoadStateView};
use std::collections::HashMap;

/// Request metadata without a slot, cancellation capability, or payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetRequestUsage {
    /// Generation targeted by this request.
    pub generation: u32,
    /// Generation active when the request began; zero means no loaded generation.
    pub from_generation: u32,
    /// Request order allocated by the originating store.
    pub sequence: u64,
    /// Whether cancellation was observed while the snapshot was created.
    pub cancelled: bool,
}

/// Outcome held by a deferred result, independent of the entry's load state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeferredAssetState {
    /// A decoded payload is retained by the deferred queue.
    Ready,
    /// A failed request is retained by the deferred queue.
    Failed {
        /// Decode, cancellation, or dependency failure.
        error: AssetError,
    },
}

/// Metadata for one result retained in the store's deferred queue.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeferredAssetUsage {
    /// Request that produced this result, including its generation.
    pub request: AssetRequestUsage,
    /// Ready or failed result, without retaining its payload.
    pub state: DeferredAssetState,
}

/// Generation and load metadata for one retained CPU store entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedAssetUsage {
    /// Loaded payload generation, including last-known-good data after failure.
    pub current_generation: Option<u32>,
    /// Latest generation requested, which may be pending or failed.
    pub requested_generation: u32,
    /// Latest load state, independent of current and retired payload ownership.
    pub state: LoadStateView,
    /// Payload generations still retained by the store, in ascending order.
    pub retired_generations: Vec<u32>,
    /// Request awaiting application; decoding may already have completed.
    pub pending: Option<AssetRequestUsage>,
}

/// CPU store ownership of one typed, normalized path identity.
///
/// This is only an inventory of store entries and deferred request identities.
/// World, history/replay, watcher, GPU cache, and payload dependency ownership
/// require separate inspection. This snapshot cannot authorize file mutations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetUsageSnapshot {
    /// Persistent typed identity; retained even when callers lose their handles.
    pub asset: AssetId,
    /// Entry metadata, absent for a deferred identity with no store entry.
    pub retained: Option<RetainedAssetUsage>,
    /// Deferred results for this identity, in their existing queue order.
    pub deferred_results: Vec<DeferredAssetUsage>,
}

impl AssetStore {
    /// Observe CPU store ownership, sorted by stable kind spelling then normalized path.
    ///
    /// Reads only metadata: does not poll/drain results, join the worker, clone
    /// payloads, or change handles or generations. Worker-channel identities
    /// remain represented by their retained entries, even after cancellation.
    /// This excludes other runtime owners and cannot authorize file mutations.
    pub fn resource_usage_snapshot(&self) -> Vec<AssetUsageSnapshot> {
        let mut identities: HashMap<_, _> = self
            .entries
            .values()
            .map(|entry| {
                let mut retired_generations: Vec<_> = entry
                    .retired
                    .iter()
                    .map(|(generation, _)| *generation)
                    .collect();
                retired_generations.sort_unstable();
                let usage = AssetUsageSnapshot {
                    asset: entry.id.clone(),
                    retained: Some(RetainedAssetUsage {
                        current_generation: entry
                            .current
                            .as_ref()
                            .map(|_| entry.current_generation),
                        requested_generation: entry.requested_generation,
                        state: entry.state.clone(),
                        retired_generations,
                        pending: entry
                            .pending
                            .as_ref()
                            .map(|pending| summarize_request(&pending.request)),
                    }),
                    deferred_results: Vec::new(),
                };
                (entry.id.clone(), usage)
            })
            .collect();
        for result in &self.deferred_results {
            let request = result.request();
            let usage =
                identities
                    .entry(request.asset.clone())
                    .or_insert_with(|| AssetUsageSnapshot {
                        asset: request.asset.clone(),
                        retained: None,
                        deferred_results: Vec::new(),
                    });
            usage.deferred_results.push(DeferredAssetUsage {
                request: summarize_request(request),
                state: match result {
                    AssetResult::Ready { .. } => DeferredAssetState::Ready,
                    AssetResult::Failed { error, .. } => DeferredAssetState::Failed {
                        error: error.clone(),
                    },
                },
            });
        }
        let mut snapshot: Vec<_> = identities.into_values().collect();
        snapshot.sort_by(|left, right| {
            left.asset
                .kind()
                .as_str()
                .cmp(right.asset.kind().as_str())
                .then_with(|| left.asset.path().cmp(right.asset.path()))
        });
        snapshot
    }
}

fn summarize_request(request: &LoadRequest) -> AssetRequestUsage {
    AssetRequestUsage {
        generation: request.generation,
        from_generation: request.from_generation,
        sequence: request.sequence,
        cancelled: request.cancel.is_cancelled(),
    }
}
