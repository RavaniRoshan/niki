# NIKI developer tasks.
#
# The serialized runner is `scripts/test-layer.sh`; this file is a thin,
# discoverable wrapper. It works without `just` installed — call the script
# directly if you prefer.

# Show available tasks.
default:
    @just --list

# ── Verification (mirrors .github/workflows/ci.yml, in order) ───────

# Format and lint. Both are CI gates and must be warning-free.
check:
    cargo fmt
    cargo fmt --check
    cargo clippy --all-targets -j 2 -- -D warnings

# Every test, serialized with a RAM preflight. This is the release gate.
test:
    ./scripts/test-layer.sh all

# Unit tests only.
test-lib:
    ./scripts/test-layer.sh lib

# Cheap integration binaries.
test-fast:
    ./scripts/test-layer.sh fast

# Binaries that build git fixtures or assert on wall-clock budgets.
test-heavy:
    ./scripts/test-layer.sh heavy

# One named binary: `just test-one run_lifecycle`
test-one name:
    ./scripts/test-layer.sh {{name}}

# Faster equivalent of `just test` when cargo-nextest is installed. nextest
# gives per-test process isolation, which removes cross-test contamination.
test-nextest:
    cargo nextest run --profile default

# ── Release gates ───────────────────────────────────────────────────

audit:
    cargo deny check
    cargo audit

# Full pre-release sequence, serialized. Long by design.
release-check: check
    ./scripts/test-layer.sh all
    cargo deny check

# ── Embedded assets ─────────────────────────────────────────────────

# Prompts and schemas are baked into the binary. Regenerate the fingerprint
# manifest after intentionally changing one.
assets:
    cargo test --test embedded_assets -- --ignored --nocapture

# ── Harness ─────────────────────────────────────────────────────────

# Install a released niki into a throwaway prefix and run it. The `install`
# CI job does the same thing; this is the local equivalent.
install-smoke:
    ./scripts/install.sh --install-dir target/install-smoke
    ./target/install-smoke/niki --version

# Visual regression gate (requires VHS + ffmpeg).
visual:
    ./tests/visual/run.sh

# Headless PTY TUI suite (requires tuiwright).
tui-headless:
    python3 -m pytest -c pytest_headless.ini -v
