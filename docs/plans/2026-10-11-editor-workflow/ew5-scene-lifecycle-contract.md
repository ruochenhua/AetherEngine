# EW5 Scene Lifecycle and Unsaved Changes Contract

Status: interaction contract reviewed; implementation slices 1–4 are in progress; file mutations remain disabled.

Implementation progress and remaining runtime-owner gaps are tracked in [EW5-B Runtime Ownership and Lifecycle Progress](ew5-b-ownership-report.md).

## Intent

Make the editor explicit about which scene is open, whether its authored contents have unsaved changes, and what happens when the user replaces or augments that scene. `LauncherState::Running` means a renderable World exists; it does not prove that a saved scene document is open.

## Reference behavior

- Unreal documents a save prompt when switching Levels. This confirms that scene replacement is an explicit save decision, not an implicit discard. [Working with Levels](https://dev.epicgames.com/documentation/unreal-engine/working-with-levels-in-unreal-engine?lang=en-US)
- Godot marks unsaved scenes in its scene tabs and asks for confirmation when closing them. Its docs establish visible dirty state and a confirmation boundary; they do not specify every dialog button. [Running code in the editor](https://docs.godotengine.org/en/stable/tutorials/plugins/running_code_in_the_editor.html), [EditorPlugin unsaved status](https://docs.godotengine.org/en/4.5/classes/class_editorplugin.html)
- Both engines distinguish bringing an asset into the project from placing/instantiating it in a scene. Unreal documents Content Browser import and viewport placement separately; Godot documents FileSystem import and dragging a model into a scene separately. [Unreal asset import](https://dev.epicgames.com/documentation/unreal-engine/importing-assets-directly-into-unreal-engine?lang=en-US), [Unreal actor placement](https://dev.epicgames.com/documentation/en-us/unreal-engine/placing-actors-in-unreal-engine), [Godot import process](https://docs.godotengine.org/en/stable/tutorials/assets_pipeline/import_process.html)

The interaction below is Aether's proposed contract, not a claim that those products use identical button labels or internal dirty-state rules.

## Document state

- The editor tracks an active document identity independently from `LauncherState` and the render World.
- The initial default-cube World and New Scene are documents named **Untitled** with a known clean baseline. New uses the same default-cube template. A failed startup scene open leaves that document available and reports the open error.
- Document state is `Clean`, `Dirty`, `Unknown`, or `RecoveryRequired`. Unknown means the agent cannot prove the current revision; RecoveryRequired means a write/load transaction reported a failed rollback or an incomplete post-load initialization. Both block scene replacement and project file mutations. Recovery UI may offer Save As recovery-copy, Reload Last Saved, or Cancel; normal Save is not claimed safe until the document is available and serializable.
- Dirtiness follows authored document changes and saved checkpoints. It does not follow selection, Project/Log browsing, preview state, or editor panel layout. Undoing to the saved checkpoint may become clean again; undo-stack depth is never the dirty oracle.
- The viewport camera is serialized into the scene on Save today. Therefore moving it marks the document dirty both while stopped and during Play. Physics-only transform churn is transient and does not mark dirty. While Playing, Save and authored scene mutations are disabled; Stop restores captured physics transforms before scene edits and saving resume. A separate authoring/play World is deferred.
- A successful Save or Save As advances the clean checkpoint and active path only after serialization and atomic file replacement succeed. Serialization stages the viewport camera without changing the live World. Cancel/failure leaves the World, prior path, and dirty state intact.
- Editor operations that persist both the scene and operation log are auto-saved edits: they remain `Clean` only after both files commit. A rollback failure becomes `RecoveryRequired` and blocks later scene writes. A later legacy edit can make the document `Dirty` again; undoing it back to the checkpoint may become `Clean`.

## Scene replacement: New, Open, and Project scene open

Clean documents proceed directly. Dirty documents pause the requested transition and show one modal with the scene name and three actions:

**Dialog copy:** “Save changes to `<scene name>` before continuing?” The modal keeps the requested action visible (for example, “Open `scenes/level.ron`”). Buttons are **Save**, **Discard**, and **Cancel**; Cancel receives initial keyboard focus. Enter activates the focused button, Escape cancels, and the modal blocks other editor controls. While it is open, New/Open/project-scene actions are disabled; a second request is ignored, not queued or substituted. A Save As picker cancellation returns to the same dirty document and cancels the pending transition.

| Action | Result |
|---|---|
| **Save** | Atomically replace the current scene path, or open Save As when no path exists. Continue only after save succeeds. A canceled Save As or failed write leaves the current document open and dirty. |
| **Discard** | Continue without saving. Keep the old World, active path, selection, history, and dirty state until the replacement candidate has loaded and initialized. If loading fails, retain the old dirty document and show the error. |
| **Cancel** | Close the modal and leave the current document untouched. |

All New/Open entry points route through the same pending transition, including the menu dialog and Project scene double-click. App/window close uses the same decision and exits only after Save or Discard completes; Cancel keeps the editor open. Project switching is not currently exposed and remains deferred. A load failure after Save leaves the old document open at the newly saved clean checkpoint; a load failure after Discard leaves the old document and its unsaved edits intact.

While Playing, New, Open, Project scene open, and app/window close are rejected with a “Stop simulation first” message; requests are not queued. Stop restores the captured physics transforms, then the user can retry the transition and receive the normal dirty prompt if needed. Import Asset to Project remains available because it does not edit the scene.

## Import and asset placement

- **Import Asset to Project** copies an external file into the project and selects it in Project. It does not modify the active scene and does not trigger the scene dirty prompt.
- **Import Scene into Current Scene** is additive. A successful import marks the scene dirty. It must be transactional: a failed import leaves World, lighting, selection, history and scene revision unchanged. AssetManager/worker/GPU caches may still retain resources loaded during preparation; those owners remain visible to the mutation inventory. Until rollback is implemented and verified, failed import yields `RecoveryRequired` and blocks scene replacement/writes with a recovery message.
- **Place asset in viewport / assign material** changes the scene while stopped. Existing editor operations persist their scene and replay log together; the edit is auto-saved and the dirty checkpoint advances only after that transaction succeeds. These operations are disabled during Play.

## Failure and recovery

- A malformed scene or resource-load failure never clears the old document, its history, path, selection, or dirty state. Prepare the candidate World, lighting, particle runtime, camera values, and pipeline requirements before commit. Switch the active World, identity, history and selection only after candidate preparation succeeds. AssetManager/cache requests made while preparing a failed candidate may remain retained; they are surfaced by the owner inventory and keep path mutation ineligible.
- Save failure never clears dirty state or changes active identity. The UI keeps the error visible and allows retry or Cancel.
- A partial import or editor-write rollback failure is not reported as clean. If rollback leaves World or disk state uncertain, enter `RecoveryRequired` and disable scene replacement and destructive file operations until the user reloads or recovers the document. A candidate initialization failure before commit leaves the old document in its prior Clean/Dirty state; do not poison a healthy old document. Save As recovery-copy may be offered only if serialization succeeds without mutating the active document.
- Runtime owner snapshots are not cleared merely because a World was replaced. They remain blockers until their owners are reconciled or invalidated by a tested lifecycle operation.

## Implementation order and acceptance

1. **Transactional scene import**: tests prove later-object failure rolls back earlier imported entities and preserves the pre-existing World, lighting, selection and history. AssetStore/cache side effects are explicitly allowed and reported separately. Defer dirty-state assertions to the checkpoint slice.
2. **Active document checkpoint model**: test clean/dirty/unknown/recovery transitions for initial/new/open, legacy edits, gizmo, viewport camera, successful scene import, undo/redo to checkpoint, auto-saved editor operations, Save/Save As success/cancel/failure, and failed open. Physics-only updates and editor selection do not dirty the document. Save and authored scene mutations remain disabled during Play; Stop restores physics transforms before they resume. Viewport camera movement still marks dirty while Playing.
3. **Shared Save/Discard/Cancel transition**: test all three actions through menu Open, Project Open, New, and app/window close, including candidate initialization failure preserving the old identity/history/path/selection, save/load failure, Escape, Enter/focus, repeated target requests, modal blocking, and cancellation of Save As. While Playing, test that each of those transition families is rejected until Stop.
4. **Runtime lifecycle/inventory gate**: reconcile all owner snapshots and refresh in-flight Project/preview work. Only after the report is complete may one-resource Move/Rename be planned again. Delete remains a later, separate contract.

No Move/Rename/Delete write is enabled by this document. The interaction contract has received independent review. Each implementation slice follows assertion-level RED then GREEN and the repository's complete verification gate. The current owner inventory is an internal read-only snapshot, not a user-facing diagnostics panel; unresolved legacy history, log receiver, and GPU material-cache ownership keep the mutation gate closed.
