# EW4 · Safe Project File Operations Contract

Status: Initial service/UI slice implemented; batch package contract extended · 2026-11-02

This brief defines the filesystem service and Project UI wiring for the first EW4 vertical slice.

## User-visible behavior

- Create a folder inside an existing `assets/` or `scenes/` project directory. The caller passes a project-relative destination and one folder name.
- Import supported assets into an existing directory below `assets/`. Multi-selection processes each selected top-level item independently so one invalid item does not cancel unrelated valid items.
- A selected folder is one package: preserve its relative directory layout, validate every imported file and supported internal dependency, then publish the whole folder or none of it. Do not leave valid siblings behind when any package member is invalid.
- Loose `.obj` files with external MTL/texture references are not complete imports; users must import them as a folder package containing the dependency files. Package OBJ material-library and diffuse-texture paths must resolve inside that package. Absolute paths, traversal, symlinks, missing references, and unsupported reference syntax are rejected.
- Ignore common Finder/Windows metadata files (`.DS_Store`, `._*`, `Thumbs.db`, `desktop.ini`, `.localized`) while importing a folder; arbitrary unsupported files fail the folder package with a readable source path.
- Destination is never inferred from the selected resource file. The UI will pass its captured current directory explicitly.
- Existing targets are never overwritten. A failed request leaves the project tree unchanged.
- New Folder opens an inline name editor; Enter/Create submits, Escape/Cancel dismisses, and blank input is a no-op.
- Import Asset opens a native multi-select picker, then a review card shows the selected sources, captured destination, and supported formats. Cancel is a no-op; only Import commits the selection.
- After successful creation/import, the project index refreshes; imported resources become selected. Errors remain visible in the editor message and structured Log panel.

## Service contract

- Add a small Launcher-side filesystem service, independent of egui and `AssetManager`.
- Revalidate project root, content root, each destination ancestor, and final path on every operation. Reject symlinks, root escapes, absolute paths, `..`, non-directory parents, and paths outside the allowlisted content roots. `assets/` and `scenes/` roots must already exist as real directories.
- Folder names must be one valid normal path component; reject empty/whitespace-only names, dot components, separators, control characters, platform-invalid filename characters, hidden names, and case-insensitive collisions.
- A loose import source must be a regular non-symlink file. A folder source must be a real non-symlink directory and is validated as a package. Validate image contents by decoding and GLB contents by parsing; extension alone is insufficient.
- GLB is self-contained only when every buffer and image source is embedded in the GLB or uses a bounded `data:` URI. Reject external URIs and report them. Require GLB magic/version/length validity and then validate against the existing engine mesh loader.
- Stage loose files and complete folder packages in unique hidden paths on the destination filesystem. Verify staged bytes and package dependencies before publication. Publish with same-filesystem exclusive rename. On any error, remove staging and preserve existing targets. Never fall back to a non-atomic copy into the final name.
- Bound imported package depth, entry count, per-file size, and aggregate size; reject non-regular entries and all symlinks.
- Return the project-relative slash-separated published path. Errors should be typed or clearly distinguish path, collision, unsupported format, external dependency, validation, and I/O failures.

## Required test-first cases

1. Folder creation succeeds under both allowed content roots and returns a normalized project-relative path.
2. Folder creation rejects invalid names, case-insensitive collisions, symlink parents, and paths outside the project; rejected requests leave no directory behind.
3. A valid PNG/JPEG import publishes identical bytes under `assets/` and does not overwrite an existing file.
4. Corrupt image bytes, mismatched extensions, unsupported formats, and external symlink sources are rejected without a final or staging file.
5. A minimal valid embedded GLB is accepted; external buffer/image URI, JSON `.gltf` data named `.glb`, malformed GLB chunks, and invalid engine mesh content are rejected with no published file.
6. Destination traversal, symlink ancestors, and case-insensitive target collisions are rejected, with the existing destination unchanged.
7. A valid folder package containing nested supported assets and OBJ dependencies publishes with relative paths intact. Missing/escaping OBJ dependencies, unsupported files, invalid bytes, symlinks, collisions, and package-budget violations publish no folder and leave no staging directory. A valid independent source in the same multi-selection still imports when another source fails.

Tests must use real temporary directories and actual file bytes. Any parser seam must exist for a production concern such as byte-budget enforcement, not merely to mock success.

## Explicitly out of scope for the initial slice

- cross-volume moves, overwrite, delete, rename, move, undoable file transactions.
- import of external-dependency glTF, loose OBJ files with external dependencies, overwrite, delete, rename, move, or undoable file transactions. Dependent OBJ assets are supported only as complete folder packages under the package rules above.
- mutation or preloading of the runtime `AssetManager`.
