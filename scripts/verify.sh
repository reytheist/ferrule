#!/usr/bin/env bash
set -euo pipefail

echo "==> Rust workspace tests"
cargo test --workspace

echo "==> Rust clippy (zero warnings)"
cargo clippy --workspace --all-targets -- -D warnings

echo "==> Frontend typecheck, lint, and build"
pnpm typecheck
pnpm lint
pnpm build

echo "==> All local checks passed."
