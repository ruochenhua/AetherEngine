# EW5-B Runtime Ownership and Lifecycle Progress

Date: 2026-10-18  
Status: Lifecycle slices 1–4 implemented as internal contracts and read-only snapshots; Move/Rename/Delete remain disabled

## Implemented in this pass

### Transactional scene import

Scene import now tracks entities created by the current attempt and removes those entities when a later object fails to build. A regression test verifies that a pre-existing World entity survives and the partially imported entity is removed. AssetStore/cache requests made while preparing the failed import can remain retained and are inventoried separately.

### Active document checkpoint and dirty state

The editor tracks document identity and a serialized clean checkpoint with `Clean`, `Dirty`, `Unknown`, and `RecoveryRequired` states. Selection and physics-only simulation churn do not dirty the document; saved viewport-camera changes do. Save serialization stages the current viewport camera without modifying the live World. Scene edits and saves are blocked while Playing, while replayable auto-saved editor operations advance their checkpoint only after persistence succeeds.

### Shared Save / Discard / Cancel transitions

New, Open, Project scene open, and application/window close use a single pending-transition model. Dirty documents use the reviewed modal contract. Open prepares and initializes a candidate World before replacing the current one, so parse/load/initialization failure keeps the prior document. Save and Save As advance identity/checkpoint only after successful atomic replacement. While Playing, transitions are rejected with Stop-first behavior and are not queued.

Focused tests cover cancel, play rejection, repeated requests, save success/failure, keyboard dismissal/focus, and transactional scene import. This implementation does not claim every matrix permutation in the contract has its own dedicated test yet; the complete CI suite remains required before calling the pass verified.

### Runtime owner snapshot and planning gate

An internal deterministic snapshot currently collects:

- active World resource references and typed editor-history resource use;
- AssetStore retained identities and GPU texture-cache inventory;
- persisted editor-log resource paths and watcher-owned paths;
- pending app requests, scene-transition state, and legacy undo/redo count;
- Project scan state, active/pending preview requests, reference-analysis state;
- animation fixture ownership and retained LogPanel event links.

The snapshot is observational: it does not drain queues or mutate owners. Unknown lifecycle state, missing inventory, active work, non-clean documents, simulation ownership, or unidentified legacy history blocks rename planning. It is an internal API and test surface today; no user-facing report panel has been added.

## Current decision: no-go for Move/Rename/Delete

The gate correctly remains closed under current runtime conditions:

- legacy undo/redo entries do not expose typed resource locators and block while present;
- the LogPanel receiver queue cannot be inspected without consuming it, so its possible resource paths are marked unobserved and block;
- a GPU material-cache inventory is not yet integrated into this snapshot;
- no user-visible diagnostic report currently explains the blockers in the editor.

These are real gaps, not permission to relax the gate. Resolve them before planning a writer. Delete remains a separate, later contract. The ownership snapshot itself does not implement path relocation or cache invalidation.

## Earlier ownership work retained

`AssetManager::usage_snapshot()` reports deterministic typed identities for retained current/retired CPU generations, pending requests, and deferred results. `capture_world_resource_usage(&World, project_root)` reports authored file meshes, material links and texture slots, prefab sources/overrides, terrain maps, and water maps while preserving path diagnostics.

- Commits: `0432d1f` `feat(asset): add read-only resource usage snapshots`; `31fb236` `feat(editor): snapshot live World resource references`; `86d1a17` `fix(editor): validate linked texture identities before normalization`
- Earlier verification: focused tests, engine tests, module health, full CI, and independent review passed for those commits.

## Next work in order

1. Complete and rerun the repository verification gates for the current lifecycle slices.
2. Reconcile the remaining owner gaps: typed legacy undo/redo locators or a safe reset contract, non-consuming log-queue inventory, and GPU material-cache inventory.
3. Add an editor-visible read-only blocker report and tests proving it reflects the captured snapshot without changing owner state.
4. Reassess readiness. Only after the complete inventory can prove quiescence should a one-resource Move/Rename contract be planned. Keep Delete deferred.
