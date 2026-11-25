# Ferrule

MIT-licensed low-memory Windows + Linux desktop email client (Tauri v2 + Rust `crates/mail-core` + SQLite/FTS5 + OS keychain + Mosaic React/TS UI).

ship-target: commit locally on master (push to origin when approved)

## Checks (local-only; no GitHub Actions CI)

- `pnpm verify` (or `./scripts/verify.ps1` / `./scripts/verify.sh`), which runs:
  - `cargo test --workspace`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `pnpm typecheck`
  - `pnpm lint` (zero warnings)
  - `pnpm build`
- Local release builds: `pnpm build:server` (`staff-sync-server`) and `pnpm tauri build` (desktop installer)

## Invariants

- Never store passwords, refresh tokens, or OAuth client secrets in SQLite or `localStorage`; all secrets go strictly through `SecretStore` (OS keychain).
- Keep `keyring = "3"` native features (`windows-native`, `sync-secret-service`, `crypto-rust`, `apple-native`).
- Keep Vite ignores for `**/target/**`, `**/src-tauri/**`, `**/crates/**` in `vite.config.ts` to prevent Windows `EBUSY` errors during `pnpm tauri dev`.

## Agent skills

### Issue tracker

Local markdown files under `.scratch/`. See `docs/agents/issue-tracker.md`.

### Triage labels

Default five-label vocabulary (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context (`GLOSSARY.md` + `docs/adr/`). See `docs/agents/domain.md`.
