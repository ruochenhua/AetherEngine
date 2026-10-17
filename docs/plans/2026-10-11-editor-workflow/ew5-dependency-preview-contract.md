# EW5-A · Typed Resource Dependency Preview

Date: 2026-10-17  
Status: Approved implementation slice; read-only

## Why this is the first slice

EW5 will eventually support dependency-aware move, rename, and delete. The current editor has no general dirty-scene flag, and the active World plus path-keyed material, asset, and hot-reload state retain the old path. A filesystem-only rename would leave editor state stale. The first slice therefore builds and verifies the dependency model without mutating project files. Later slices can add operations only after they can update authored documents and editor runtime state transactionally.

## Interaction

- Select one indexed resource in Project and choose **Show References** from its context menu or details area.
- The panel pins the analyzed target independently from Project's current selection. Its header always names that target; selecting a reference row must not retarget the analysis. Provide **Back to target**.
- The panel displays **Used by** inbound references and **Depends on** outbound resources, with the source document, typed field, and resolved/missing status.
- Selecting a file-backed reference row selects that document in Project and navigates to its directory; it does not open a scene/resource or change Scene entity selection. Missing/non-file references show their exact reason and are not navigable.
- Analysis is read-only. Close/Escape dismisses it without changing selection, disk, logs, or history. Unsupported or malformed reference-bearing documents appear as diagnostics; the UI must say that the result is incomplete.
- Show Loading, Complete, Incomplete, Failed, Stale, and Empty states. Analysis runs asynchronously; if source fingerprints change while the report is open, label it Stale and offer Reanalyze. This is an explicit snapshot, not a live watcher.
- No editing keyboard shortcuts are handled by this panel. Escape closes it only when the panel owns focus; text controls keep normal input behavior.
- No Move, Rename, or Delete command is enabled in EW5-A. The preview must not imply that an analyzed resource is safe to mutate.

## Typed reference coverage

Inspect authored project-relative references in:

- Scene objects: file meshes, MaterialAsset links, inline material texture slots, and transparent texture.
- Scene Prefab instances and their typed overrides.
- Scene Terrain: heightmap, splatmap, and all layer textures.
- Scene Water: DuDv and normal maps.
- Prefab documents: nested node mesh sources and inline material/texture overrides. The current Prefab schema has no MaterialAsset link field.
- MaterialAsset documents: every texture slot, resolved relative to the material document's directory as the current loader does.

References retain their semantic kind and stable source locator: Scene object index/name and field, Scene Prefab instance id/override index/field, Prefab node instance id/component/slot, or MaterialAsset path/slot. Duplicate edges are preserved when distinct fields point to the same resource. The visitor must not perform string search/replace or normalize a reference differently from the owning loader.

Scene and inline Prefab material texture paths resolve from the project root. MaterialAsset texture paths resolve from the MaterialAsset document's parent directory. Other authored resource paths are project-root-relative. Preserve both authored spelling and normalized project-relative candidate when validation fails. Terrain heightmap strings are recorded as authored references even though current geometry generation does not load them yet.

## Failure and completeness

- Walk only the existing allowlisted `assets/` and `scenes/` roots and reject/diagnose symlink or out-of-root paths using the same containment rules as Project indexing.
- Parse supported document kinds with their real schema validators. A known reference-bearing document that is unreadable, malformed, oversized, or has an unsupported schema yields a blocking diagnostic for any future mutation preview.
- Preserve missing references as explicit unresolved edges; do not silently drop them.
- Unknown `.ron` documents under allowlisted roots are blocking diagnostics. Identify supported Scene, Prefab, and MaterialAsset schemas independently from ProjectIndex's heuristic; in particular, scalar-only and transparent-texture-only MaterialConfig documents are supported candidates. Supported Prefab v1 migration is accepted; unsupported versions block.
- Ordinary binary/image payloads are graph endpoints, not documents. Authored-reference formats not yet visited (including OBJ and external-resource glTF) are listed as out-of-model blockers when present; never treat them as proof of completeness.
- Descendant symlinks skipped by ProjectIndex must be detected and surfaced as blockers here. Canonical/out-of-root reference candidates retain the authored spelling and a diagnostic.
- Fingerprint every file's normalized relative path and full bytes, including the selected resource and all allowlisted inventory files; include directories and add/remove inventory so target replacement or newly added documents change the aggregate fingerprint. Hash files as a stream under explicit file-count, per-file, and total-byte budgets. If a file changes during the scan, mark the result stale/incomplete or retry.
- A future writer must compare the target and every changed reference document's fingerprints and the aggregate inventory before staging, then recheck before publication.
- Keep deterministic ordering by source path, field kind, and target path.
- While running, future path mutations are blocked until all live World, AssetManager, material/texture caches, and watcher path state can be migrated or safely reset. Dirty-scene state is Unknown and blocks all affected mutations until explicit Save/Cancel semantics exist; do not infer “saved” from the lack of a dirty flag.

## RED tests

1. Collect project-relative mesh and material references from multiple Scene objects without changing the parsed document.
2. Collect every MaterialConfig texture slot, including transparent texture, with distinct semantic field kinds.
3. Collect Terrain heightmap/splatmap/layer references and Water maps.
4. Collect nested Prefab mesh/material references and Scene Prefab-instance overrides.
5. Resolve MaterialAsset texture paths relative to its own directory, including normalized `..` that remains inside the allowlisted root; report escapes and symlinks as diagnostics.
6. Preserve missing targets as unresolved references and report malformed/unsupported schema documents as incomplete diagnostics.
7. Ignore unrelated non-reference fields that happen to contain path-looking strings.
8. Return the same graph ordering across filesystem enumeration orders and leave all files byte-identical.
9. Mark malformed and unclassified RON documents as blockers rather than reporting a complete empty graph.
10. Change a source fingerprint when either the resource or a reference document changes.
11. Confirm that opening/closing analysis leaves file bytes, scene World, AssetManager state, Project selection, and index generation unchanged. Clicking an edge may navigate/select its source document but must preserve Scene entity selection and the pinned analysis target.
12. Keep the analyzed target pinned after selecting a reference row; Back to target restores Project navigation without recomputing unless stale.
13. Show deterministic empty/incomplete/stale/error states and never display an incomplete report as “unreferenced.”
14. Hash same-size content changes, target replacement/deletion, and file/directory inventory add/remove; detect symlinks even though ProjectIndex excludes them from its snapshot.
15. Distinguish endpoints from reference documents: unknown `.ron` and unvisited authored-reference formats block completeness, while ordinary binary/image payloads do not.

## Next EW5 slices

- EW5-B: safe rename/move for one resource with a complete impact preview, exact typed document rewrites, same-volume staging, collision checks, and rollback tests. Block dirty-scene/runtime references until editor state can be reconciled.
- EW5-C: delete only when the dependency graph is complete and no authored/runtime references remain; initially permit empty directories only. Use a recoverable project-local quarantine if delete undo is promised.
- Folder moves, multi-select, cross-volume operations, overwrite, forced deletion, and best-effort partial commits remain out of scope until separately designed.
