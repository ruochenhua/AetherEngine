#!/usr/bin/env bash
# Aether Engine CI-style verification script.
#
# Runs the same checks a human/CI should run before merging:
#   - formatting
#   - clippy warnings as errors
#   - verification report format
#   - all workspace tests
#   - module health / file-size governance
#   - release build of the whole workspace
#
# Usage:
#   ./scripts/verify-ci.sh
#
# The visual regression matrix is intentionally not part of this fast CI path.
# Run ./scripts/verify-regression.sh separately when rendering is affected.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

step() {
    echo ""
    echo "=== $1 ==="
}

fail() {
    echo ""
    echo "❌ $1" >&2
    exit 1
}

step "cargo fmt --check"
if ! cargo fmt --check; then
    fail "cargo fmt --check failed; run 'cargo fmt' first"
fi

step "cargo clippy --workspace --all-targets -- -D warnings"
if ! cargo clippy --workspace --all-targets -- -D warnings; then
    fail "cargo clippy failed"
fi

step "./tests/report-format-test.sh"
if ! ./tests/report-format-test.sh; then
    fail "verification report format check failed"
fi

step "./tests/metal-regression-runner-test.sh"
if ! ./tests/metal-regression-runner-test.sh; then
    fail "Metal regression runner flow check failed"
fi

step "./tests/runner-process-test.sh"
if ! ./tests/runner-process-test.sh; then
    fail "runner process contract check failed"
fi

step "cargo test --workspace"
if ! cargo test --workspace; then
    fail "cargo test failed"
fi

step "./scripts/verify-module-health.sh"
if ! ./scripts/verify-module-health.sh; then
    fail "module health check failed"
fi

step "cargo build --workspace --release"
if ! cargo build --workspace --release; then
    fail "cargo build --release failed"
fi

echo ""
echo "✅ All CI checks passed"
