# EW2 · Read-only Project browser

**Status:** Ready for implementation · **Depends on:** EW0; EW1 supplies diagnostics

## User outcome

From the startup menu or running editor, the user can browse project scenes and assets, search/filter them, inspect basic metadata, and open a scene. This task is read-only; it does not rename, move, import, copy, or delete files.

## Index contract

- Scan only `<project_root>/scenes` and `<project_root>/assets` for this first version. A missing root is an empty/error state; do not silently fall back to scanning the whole checkout.
- Reject a content root if the root itself is a symlink. Do not traverse symlink entries. Verify canonical paths remain beneath the canonical project root.
- Ignore hidden paths, `.git`, `target`, `tests/reports`, and cache/output directories.
- Produce immutable, deterministically sorted snapshots. A scan has a monotonically increasing request generation; a late result cannot replace a newer snapshot. A failed refresh retains the last successful snapshot and exposes an error state.
- Recognize scenes, prefab documents, material assets, glTF, OBJ, and supported image formats. Classify RON with strict schema/content evidence plus the containing content root. Unknown, malformed, and ambiguous RON remain Unknown/Invalid; do not infer Material from default-deserializable fields.
- Project indexing reads filesystem metadata and small type headers only; it does not eagerly decode all image/model resources or create GPU objects.

## UI contract

- Directory tree; breadcrumb and back/forward path history; one stable list view; name search, resource-kind filter, sorting; explicit scanning/empty/error/no-results states.
- Menu and Running share the same selected path and Project state.
- Single selection shows path/type/size/status and supported light metadata preview; double-click opens only valid scenes in the first version.
- An invalid scene open keeps the existing scene/world intact and sends a structured error to EW1.
- Asset selection does not clear the currently selected scene entity.

## TDD acceptance

Use temporary directory fixtures and write/observe RED tests before scanner/UI production code:

1. Allowlisted root scan returns mixed resource types in stable order; search, filter, breadcrumb history are deterministic.
2. Symlink content roots are rejected; nested symlinks and hidden/ignored folders are not traversed; lexical and canonical escapes are rejected.
3. Valid scene/prefab/material fixtures classify correctly; blank/defaultable or ambiguous RON stays Unknown/Invalid.
4. Add/modify/delete plus malformed file refresh updates state; failed scan keeps last good snapshot; late older request cannot overwrite newer result.
5. Selecting resource details preserves scene selection; opening valid scene succeeds; opening invalid scene preserves the previous world and creates a queryable error event.
6. UI keyboard focus prevents scene Delete shortcut while Project search/rename widgets own focus.

## Deliverables

- Testable scanner/index model, UI query state, menu/running integration, scene open action.
- No write operations; these are deferred to EW4/EW5 after separate reference/import design.
- RED/GREEN evidence, launcher release startup and scene-open interaction record, and full governance checks.
