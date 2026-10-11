# EW1 · Structured logging and Log panel

**Status:** Ready for implementation · **Depends on:** EW0

## User outcome

The user can inspect launcher and engine diagnostics inside the editor, filter them, and find asset/scene context without opening a terminal. Logs continue to be collected in `--no-gui-overlay` runs. Editor transaction history in `.editor.jsonl` remains a distinct artifact.

## Design contract

- Define `CapturedEvent` with timestamp, level, target/category, message, structured fields, and only validated project-relative resource context.
- Keep pure bounded `LogStore` behavior separate from subscriber and file I/O. UI and file sinks have independent bounded queues.
- Event callbacks never wait for egui or disk. Queue overflow can discard any event; track dropped counts by level and expose sink health. Do not claim lossless ERROR durability under overflow or I/O failure.
- File writer owns output, rotation, retention, and shutdown flush. Write failure emits at most one direct stderr summary from the writer and sets degraded health; never call tracing recursively from a tracing Layer.
- `Clear` clears only the UI history; `Pause` only stops auto-scroll; filters only affect the view. None stops collection or deletes persisted files.
- Install the subscriber once before CLI parsing as currently required. Tests use thread-local `tracing::subscriber::with_default`, not global init.

## TDD acceptance

Write the smallest behavioral tests first and observe each expected RED before implementation:

1. Capture a real structured event and preserve timestamp, level, target, message, and fields.
2. Concurrent producers do not block on the UI consumer; queue capacity stays fixed and overflow counters match dropped events.
3. Level/category/text filters change visible results only; clear, pause-auto-scroll, and resume preserve collection semantics.
4. A bounded file sink writes admitted records; a full queue increments level counters and reports degraded health.
5. An unwritable directory reports a stable error and emits a single stderr fallback without recursion.
6. Normal shutdown flushes all admitted records; rotation/retention bounds file size and count.
7. A launcher started with `--no-gui-overlay` still records logs.
8. A structured project-relative `asset`/`path` field resolves to Project selection; arbitrary text and paths escaping root do not become clickable links.

## Scope limits

- No interactive command console in this task.
- Do not combine runtime diagnostics with `.editor.jsonl` operation replay.
- Avoid rendering/logging every frame by default; user-facing levels start at INFO, with a developer setting/CLI route for DEBUG.

## Deliverables

- Bounded LogStore and bounded nonblocking sinks, panel filters/details, persistent session log, health/drop counters.
- RED/GREEN evidence for every acceptance behavior.
- Release checks for normal UI and no-GUI logging, plus clean shutdown/flush evidence.
