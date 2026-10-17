# EW4 · Safe Project File Operations Contract

Status: Service and Project UI vertical slice implemented · 2026-10-17

This brief defines the filesystem service and Project UI wiring for the first EW4 vertical slice.

## User-visible behavior

- Create a folder inside an existing `assets/` or `scenes/` project directory. The caller passes a project-relative destination and one folder name.
- Import exactly one `.png`, `.jpg`, `.jpeg`, or self-contained `.glb` into an existing directory below `assets/`.
- Destination is never inferred from the selected resource file. The UI will pass its captured current directory explicitly.
- Existing targets are never overwritten. A failed request leaves the project tree unchanged.
- New Folder opens an inline name editor; Enter/Create submits, Escape/Cancel dismisses, and blank input is a no-op.
- Import Asset opens a native single-file picker, then a review card shows source, captured destination, and supported formats. Cancel is a no-op; only Import commits the file.
- After successful creation/import, the project index refreshes; imported resources become selected. Errors remain visible in the editor message and structured Log panel.

## Service contract

- Add a small Launcher-side filesystem service, independent of egui and `AssetManager`.
- Revalidate project root, content root, each destination ancestor, and final path on every operation. Reject symlinks, root escapes, absolute paths, `..`, non-directory parents, and paths outside the allowlisted content roots. `assets/` and `scenes/` roots must already exist as real directories.
- Folder names must be one valid normal path component; reject empty/whitespace-only names, dot components, separators, control characters, platform-invalid filename characters, hidden names, and case-insensitive collisions.
- Import source must be a regular non-symlink file. Validate its format by decoding an image or parsing a binary GLB; extension alone is insufficient.
- GLB is self-contained only when every buffer and image source is embedded in the GLB or uses a bounded `data:` URI. Reject external URIs and report them. Require GLB magic/version/length validity and then validate against the existing engine mesh loader.
- Stage the copy in a unique hidden path on the destination filesystem. Verify the staged copy and publish with same-filesystem rename. On any error, remove staging and preserve existing targets. Never fall back to a non-atomic copy into the final name.
- Return the project-relative slash-separated published path. Errors should be typed or clearly distinguish path, collision, unsupported format, external dependency, validation, and I/O failures.

## Required test-first cases

1. Folder creation succeeds under both allowed content roots and returns a normalized project-relative path.
2. Folder creation rejects invalid names, case-insensitive collisions, symlink parents, and paths outside the project; rejected requests leave no directory behind.
3. A valid PNG/JPEG import publishes identical bytes under `assets/` and does not overwrite an existing file.
4. Corrupt image bytes, mismatched extensions, unsupported formats, and external symlink sources are rejected without a final or staging file.
5. A minimal valid embedded GLB is accepted; external buffer/image URI, JSON `.gltf` data named `.glb`, malformed GLB chunks, and invalid engine mesh content are rejected with no published file.
6. Destination traversal, symlink ancestors, and case-insensitive target collisions are rejected, with the existing destination unchanged.

Tests must use real temporary directories and actual file bytes. Any parser seam must exist for a production concern such as byte-budget enforcement, not merely to mock success.

## Explicitly out of scope for slice A

- multi-file import, dependency bundles, overwrite, delete, rename, move, undoable file transactions.
- import of OBJ, external-dependency glTF, multi-file dependency bundles, overwrite, delete, rename, move, or undoable file transactions.
- mutation or preloading of the runtime `AssetManager`.
