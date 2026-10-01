# T8 Implementation Archive

Date: 2026-10-01

Status: **T8.1–T8.4 complete**

This archive links the passing primary-case reports for the four T8 slices. Each linked run preserves its HTML report, machine-readable results, logs, and any scene capture produced by the runner.

| Slice | Run | Result | Evidence |
|---|---|---|---|
| T8.1 Typed AssetStore | `t8-20261001T062844Z-16` | 4/4 probes passed; fixture exited 0. This case does not launch a scene. | [HTML report](../../../tests/reports/t8-20261001T062844Z-16/t8_asset_store_lifecycle/report.html) |
| T8.2 MaterialAsset reload | `t8-20261001T063913Z-1027` | 3/3 probes passed; fixture and one-frame Launcher scene exited 0. | [HTML report](../../../tests/reports/t8-20261001T063913Z-1027/t8_material_reload_lkg/report.html) |
| T8.3 Prefab round-trip | `t8-20261001T083138Z-1923` | 4/4 probes passed; fixture and one-frame Launcher scene exited 0. | [HTML report](../../../tests/reports/t8-20261001T083138Z-1923/t8_prefab_roundtrip/report.html) |
| T8.4 Hot reload and shutdown | `t8-20261001T091626Z-163` | 3/3 probes passed; fixture and one-frame Launcher scene exited 0. | [HTML report](../../../tests/reports/t8-20261001T091626Z-163/t8_hot_reload_shutdown/report.html) |

The T8.2–T8.4 reports include a rendered screenshot. No approved golden reference was configured for these cases, so image-difference comparison was not run; the screenshots are retained as visual run evidence only. The bounded scene runner exited after capture, so no test window was left open.

The report metadata records the repository `HEAD` available to the runner. T8.1 and T8.2 therefore reference their respective earlier implementation commits. The T8.3 and T8.4 runs were executed on the working tree based on `de991f64dfeca275779cd6ce14109d252adff26e`; that runner does not encode uncommitted diff contents in its commit field. The source changes validated by those runs are archived with this record.

## Workspace verification

`./scripts/verify-ci.sh` passed formatting, Clippy, the Metal runner smoke test, `cargo test --workspace` (including 355 engine library tests and 51 Launcher tests), and integration/doc tests. The report-format and process-isolation tests were skipped because this Windows host does not support the runner's POSIX `fork` isolation. The script then exited at `verify-module-health.sh`: it found known baseline-growth entries, while confirming that no Rust file exceeds 500 lines. The baseline was left unchanged. The release Launcher builds used by T8.2–T8.4 each captured their scene and exited with code 0.
