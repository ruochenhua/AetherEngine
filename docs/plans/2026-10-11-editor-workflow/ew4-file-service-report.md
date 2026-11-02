# EW4 slice A filesystem service implementation report

Date: 2026-10-17

Implemented Launcher module `project_files`, independent of egui and AssetManager. Public entry points:

- `create_folder(root: &Path, directory: &str, name: &str) -> Result<String, FileOperationError>`
- `import_file(root: &Path, directory: &str, source: &Path) -> Result<String, FileOperationError>`

Successful operations return project-relative slash-separated paths. Typed errors distinguish Path, Collision, UnsupportedFormat, ExternalDependency, Validation and Io; Display messages are suitable for UI presentation.

Folder requests validate a real existing root, content root and each existing destination directory; assets/ and scenes/ are allowed. Imports accept assets/ destinations only. Both reject lexical traversal, absolute paths, symlinks in destination components, invalid filename characters, hidden names, Windows reserved filenames and case-insensitive collisions. Final name checks run again before publication.

Imports require a regular non-symlink source, cap source bytes at 64 MiB, check actual PNG/JPEG signature against extension and decode images with a 64 MiB allocation limit. GLB checks cover binary magic/version/declared length, chunk boundaries/order/alignment, glTF document validation, buffer and image dependency URI inspection, bounded base64 data URIs and the existing engine mesh loader. Loader panics from untrusted mesh conversion become Validation failures.

Bytes are written to a create-new hidden staging file in the destination, synced, read back and validated. Engine GLB mesh loading runs against that staged file. An RAII guard removes staging on every error, including post-staging loader rejection. Publication uses exclusive same-filesystem rename (Darwin renamex_np RENAME_EXCL, Linux renameat2 RENAME_NOREPLACE, Windows rename); unsupported platforms fail instead of copying or using overwrite-prone Unix std::fs::rename.

## TDD evidence

Tests were written before production implementation, using real temporary directories and real PNG/JPEG/GLB bytes. Public API scaffolding returned `Validation("not implemented")` to make initial tests compile.

1. RED: `cargo test -p aether-launcher project_files::tests --lib`
   - Result: 9 failed, 1 passed; expected missing implementation failures. The generic malformed-GLB rejection case already matched the scaffolding error, while successful operations, typed path/collision/format/dependency rejection cases all failed.
   - Captured output: `/tmp/ew4-red.log`.
2. First GREEN attempt encountered concurrent parent UI edits (`WorkspaceRequests` vs `Option` mismatches); no service compile error.
3. GREEN: `cargo test -p aether-launcher project_files::tests --lib`
   - Result: 10 passed, 0 failed. At that point the concurrent parent wiring still produced two unrelated temporary dead-code warnings for refresh methods.
   - Captured output: `/tmp/ew4-green.log`.
4. `./scripts/verify-module-health.sh`
   - Initial result: failed due to concurrent `app/scene/ops.rs` reaching 558 lines. Service-owned files were all below 500 lines (largest tests.rs: 310 lines). Parent informed; no baseline adjustment.

The tests cover both folder roots, invalid names, traversal/non-directory paths, case collisions, real image bytes with no overwrite, corrupt/mismatched/unsupported files, embedded triangle GLB import, external buffer/image URI rejection, malformed chunks/header/version, JSON disguised as GLB, engine no-scene failure cleanup, source/destination/root symlinks and symlink content roots.

## Boundaries and remaining verification

No UI wiring or runtime asset registration was changed by this slice. Parent owns integrated UI validation, final workspace CI and runtime editor launch.

Filesystem component validation and the exclusive rename prevent ordinary invalid requests and exact-name overwrites. The service does not claim isolation from an adversarial external process swapping directory ancestors between validation and filesystem calls, or concurrently creating a differently-cased filename on a case-sensitive filesystem. Addressing that stronger threat model requires directory-handle-relative operations and cross-process namespace coordination. Ordinary failure cleanup is covered; abrupt process termination can leave a hidden staging file, as with other filesystem staging protocols.

Linux and Windows exclusive rename paths are compiled only on those targets; this session runs Darwin. No fallback copy is used.

## Final focused verification

- `rustfmt --edition 2021 crates/aether-launcher/src/project_files.rs crates/aether-launcher/src/project_files/tests.rs crates/aether-launcher/src/project_files/validate.rs crates/aether-launcher/src/project_files/publish.rs`: complete.
- `cargo clippy -p aether-launcher --all-targets -- -D warnings`: passed, no warnings (`/tmp/ew4-clippy.log`).
- `cargo test -p aether-launcher project_files::tests --lib`: passed, 10/10 tests, no warnings (`/tmp/ew4-green-final.log`).
- `./scripts/verify-module-health.sh`: passed after parent split, 388 Rust files checked, 0 over-limit files.

## Folder package transaction follow-up · 2026-11-02

Closed the remaining package-integrity gap found during the EW4 contract audit:

- A selected folder is collected and checked before publication. Any unsupported entry, invalid asset, missing OBJ material/texture dependency, symlink, collision, or package-budget violation rejects the folder as a whole.
- Files are copied into a hidden sibling staging directory, byte-checked, dependency-checked, and published with the existing exclusive same-filesystem rename. An RAII guard removes the staged tree on failure.
- Separate top-level selections remain independent, so a failed folder does not cancel a valid selected file.
- OBJ dependency validation is shared by the importer and viewport placement path. The importer validates the engine OBJ loader against the staged `.obj` path after all package files exist.
- Package limits are 32 directory levels, 4096 entries, 64 MiB per file, and 512 MiB aggregate. Common Finder/Windows metadata is skipped; other unsupported entries fail the package.

TDD evidence:

1. RED: rollback, missing-dependency, and nested-symlink tests failed against the previous per-file folder importer, which left a partially published folder. Valid nested OBJ/MTL/PNG and traversal rejection were added as package contract regressions.
2. GREEN: `cargo test -p aether-launcher --lib project_files::tests` passed, 18/18.
3. One full CI attempt hit an existing timing-sensitive hot-reload test; the isolated rerun passed, and the subsequent full workspace run passed.
4. Final `./scripts/verify-ci.sh` passed, including workspace Clippy/tests, module health (433 Rust files, 0 over-limit), and workspace Release build.

The package importer has automated service and UI routing coverage. The new OBJ dependency-package path has not yet received a separate manual editor acceptance session. The release launcher was also started with `scenes/test_one_cube_far.ron` for a short smoke run and stayed alive without panic; visual inspection was unavailable because the desktop automation service could not start.
