#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"

Write-Host "==> Rust workspace tests"
cargo test --workspace

Write-Host "==> Rust clippy (zero warnings)"
cargo clippy --workspace --all-targets -- -D warnings

Write-Host "==> Frontend typecheck, lint, and build"
pnpm typecheck
pnpm lint
pnpm build

Write-Host "==> All local checks passed."
