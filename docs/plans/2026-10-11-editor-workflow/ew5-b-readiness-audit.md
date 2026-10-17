# EW5-B Readiness Audit

Date: 2026-10-17  
Status: No-Go for enabling Move/Rename writes in the current editor

## Decision

EW5-A's authored dependency report is necessary but not sufficient to authorize a filesystem mutation. Do not enable Move/Rename/Delete until runtime path owners and unsaved scene state have an explicit eligibility/reconciliation contract. Three independent read-only audits reviewed interaction, runtime ownership, and transaction failures.

## Verified runtime blockers

- **There is no useful offline Menu state in normal startup.** `app/handler.rs` creates a default World and switches to Running when the window resumes; a Menu-only allowlist would not be reachable through the normal desktop flow.
- **Scene dirty state is unknown.** World edits, camera persistence, editor history and save behavior do not share a saved baseline. Undo depth is not a clean/dirty signal.
- **Path references outlive the current World.** AssetStore retains entries, path-derived IDs, pending worker jobs and deferred results across scene changes. It exposes no complete loaded/pending path inventory, eviction or relocation operation.
- **Other runtime owners retain paths or identities.** Material texture dependencies, GPU texture caches, hot-reload watcher queues, Undo/Redo runtime states, persisted `.editor.jsonl` replay records, scene-load requests, resource drops and operation queues all need inspection or invalidation.
- **EW5-A completeness describes authored Scene/Prefab/MaterialAsset and supported glTF graph coverage.** It does not certify replay history, active World state, caches, pending jobs or watcher state.
- **MaterialAsset relocation also rebases its outbound texture paths** because the loader resolves them relative to the material's directory. Updating inbound references alone is insufficient.

The code-level audit details and source locations are recorded in the review notes delivered during this task. Relevant implementation owners include `app/handler.rs`, `asset/store.rs`, `asset/material_asset.rs`, `asset/texture_cache/cache.rs`, `app/asset_hot_reload.rs`, `app/material_reload.rs`, `editor/history.rs`, `editor/operation.rs`, `app/editor/replay.rs`, and `app/workspace/project_panel.rs`.

## Interaction findings for a future write flow

A proposed Rename/Move flow should capture the source path at initiation, collect edits into a draft, and use Enter to open a pinned impact review rather than committing immediately. The review should show the resource relocation and every exact typed before/after field change, keep Cancel available before publication, and require a second explicit confirmation after a stale reanalysis. After success it should refresh/select the new path without changing Scene entity selection.

F2 is already consumed by a global debug overlay shortcut. A future Rename gesture must be scoped to a focused Project resource row and consumed before global debug input. Current rows have no context menu; “Show References” currently lives in resource details.

## Transaction and failure constraints

A future writer must re-evaluate the plan from current schemas rather than trusting caller-supplied graph edges. It must compare the complete inventory and every changed input before staging and before publication, reject collisions/symlinks/cross-volume boundaries, preserve original bytes until commit is verified, and distinguish verified rollback from `RecoveryRequired`. Multiple file renames are not globally atomic. Ordinary failure can be reported only after all original files are restored; rollback failure must preserve recovery data and block subsequent writes.

Do not claim byte-identical inode replacement detection from content hashes alone. If the writer depends on file identity, capture and recheck identity/ancestor ownership separately.

## Recommended next implementation sequence

1. Design a read-only runtime mutation-eligibility snapshot and an explicit editor/project lifecycle. Unknown dirty state remains a blocker; do not infer cleanliness.
2. Add owner APIs and tests for World, histories/replay, asset jobs/dependencies/caches, watchers and pending requests. Decide whether safe writes require a truly unloaded project or a supported live migration path.
3. Only after the eligibility report can prove safety, implement one narrow Move/Rename slice with typed rewrites, transaction journal/rollback and focused failure-injection tests.
4. Keep Delete after the move transaction is proven; keep EW6 preview tabs after EW5 path identity updates are defined.

The proposed interaction and transaction notes are design input, not an approved write-operation contract. No Move/Rename/Delete command is enabled by this audit.
