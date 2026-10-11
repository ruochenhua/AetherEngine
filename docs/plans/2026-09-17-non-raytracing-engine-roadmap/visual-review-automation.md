# Visual candidate review automation

The visual runner creates captures and per-run reports. This helper gathers the latest valid captures that have no approved reference into a review bundle, grouped by task. It writes `index.html`, `bundle.json`, contact sheets, and a `review-ledger.json` beside the output. The bundle is local evidence; `tests/reports/` is ignored by Git.

Build or refresh a bundle:

```sh
python3 scripts/visual_review_bundle.py --out tests/reports/visual-review-local
```

Review a card in the HTML page, then write the decision using the case ID and full SHA-256 shown there:

```sh
python3 scripts/visual_review_bundle.py --out tests/reports/visual-review-local \
  --review-case t4_transparent_overlap --sha256 SHA256_FROM_PAGE \
  --status pass --note "透明叠加和背景都清晰可辨"
```

Allowed decisions are `pass`, `fail`, and `insufficient`. Every decision requires a note. The decision is bound to the image hash; after a new capture changes the hash, rebuild the bundle and review that image again. Only current human `pass` entries that came from a provenance-checked `candidate.json` are eligible for the explicit promotion command:

```sh
python3 scripts/visual_review_bundle.py --out tests/reports/visual-review-local \
  --promote-approved --reason "人工逐项审核通过"
```

Promotion still passes through `run_primary_case.py`, which checks runtime provenance and candidate immutability before changing a reference. Ordinary regression captures can be reviewed in the bundle, but cannot be promoted. Machine capture, image metrics, and report generation never create a human acceptance record.

The helper has a CPU-only test suite:

```sh
python3 tests/visual_review_bundle_test.py
```
