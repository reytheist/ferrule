# Ferrule

MIT-licensed low-memory personal Windows + Linux desktop email and calendar client (Tauri v2 + Rust `crates/mail-core` + SQLite/FTS5 + OS keychain + Mosaic React/TS UI), designed for power-user email workflows and integration with a Hermes-like personal AI agent system.

ship-target: commit locally on master (push to origin when approved)

## Checks (local-only; no GitHub Actions CI)

- `pnpm verify` (or `./scripts/verify.ps1` / `./scripts/verify.sh`), which runs:
  - `cargo test --workspace`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `pnpm typecheck`
  - `pnpm lint` (zero warnings)
  - `pnpm build`
- Local desktop build: `pnpm tauri build` (or `pnpm tauri dev` for development)

## Invariants

- Never store passwords, refresh tokens, API keys, webhook secrets, or OAuth client secrets in SQLite or `localStorage`; all secrets go strictly through `SecretStore` (OS keychain).
- Keep `keyring = "3"` native features (`windows-native`, `sync-secret-service`, `crypto-rust`, `apple-native`).
- Keep Vite ignores for `**/target/**`, `**/src-tauri/**`, `**/crates/**` in `vite.config.ts` to prevent Windows `EBUSY` errors during `pnpm tauri dev`.
- Keep all IMAP/SMTP provider presets (iCloud, Yahoo, Fastmail, Zoho, GMX, IONOS, custom) in `src/features/accounts/imapPresets.ts`.

## Versioning

- Stay on `0.x.y` until the owner explicitly decides Ferrule is feature-complete for a `1.0.0` release. Never bump to `1.0.0` without explicit owner approval.
- **Patch (`x.y.Z`):** bug fixes, UI polish, IMAP/SMTP/parser fixes, security hardening, dependency/performance updates, and non-breaking SQLite indexes.
- **Minor (`x.Y.0`):** new user-facing features or roadmap phases, new additive `ferrule mcp` tools or SSE/webhook event types, additive SQLite migrations (upgrading `ferrule.db` in place), and (pre-1.0) deliberate scope pivots/removals that preserve existing personal accounts and mail.
- **Major (`X.0.0`, post-1.0):** breaking changes to `ferrule mcp` tool contracts, SSE/webhook payloads or auth/signature format, breaking SQLite or OS keychain (`ferrule:v1:...`) migrations, or dropping a supported platform/runtime.
- Keep the version synchronized across `Cargo.toml`, `Cargo.lock` (via `cargo check --workspace`), `package.json`, and `src-tauri/tauri.conf.json`.

## Agent skills

### Issue tracker

Local markdown files under `.scratch/`. See `docs/agents/issue-tracker.md`.

### Triage labels

Default five-label vocabulary (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context (`GLOSSARY.md` + `docs/adr/`). See `docs/agents/domain.md`.
