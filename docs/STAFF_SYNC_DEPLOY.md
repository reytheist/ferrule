# Deploying `staff-sync-server`

Pigment Staff Sync can run in two modes:

1. **Pigment Vercel + Postgres (recommended for Pigment Staff)**: Hosted directly inside the `pigment2027` Next.js/Vercel app at `https://pigmentshow.us/api/ferrule` using `pigment2027`'s Neon PostgreSQL database (`FERRULE_STAFF_SYNC_TOKENS` env var). Its `/v1/events` endpoint returns `204 No Content` so Ferrule Desktop automatically uses lightweight 20-second incremental polling (`GET /v1/snapshot?since=...`) without burning Vercel Fluid Compute on idle SSE sockets. Staff downloads live at `https://pigmentshow.us/ferrule` (Clerk staff-only).
2. **Standalone `crates/staff-sync-server` (self-hosted SQLite + live SSE)**: A lightweight HTTP + SSE + SQLite companion binary with zero external database or SaaS dependencies that runs comfortably in 64-128 MB RAM.

## Configuration

| Environment variable | Required | Default | Description |
|---|---|---|---|
| `STAFF_SYNC_TOKENS` | Yes (or `STAFF_SYNC_TOKEN`) | - | Comma-separated list of high-entropy staff Bearer tokens (at least one required). |
| `STAFF_SYNC_BIND` | No | `127.0.0.1:8787` | Listen socket address (`0.0.0.0:8787` inside containers). |
| `STAFF_SYNC_DB` | No | `staff-sync.db` | Path to the SQLite database file (mount on a persistent volume). |

Generate a strong token:

```bash
openssl rand -hex 32
```

### Health Check

`GET /health` is intentionally unauthenticated and returns HTTP `200` with `{"status":"ok"}` for Fly.io, Docker, or reverse-proxy probes. All `/v1/*` routes require `Authorization: Bearer <token>`.

## Local Quick Start

```bash
STAFF_SYNC_TOKENS="dev-token-change-me" \
STAFF_SYNC_DB="./staff-sync.db" \
cargo run -p staff-sync-server --bin staff-sync-server
```

In Ferrule Desktop (**Settings -> Pigment Staff Sync**), connect to `http://127.0.0.1:8787` with `dev-token-change-me`. For remote servers, Ferrule requires `https://`.

## Docker (Local or Small VPS)

Build the multi-stage image from the repository root:

```bash
docker build -t ferrule-staff-sync .
```

Run with a persistent named volume for SQLite:

```bash
docker volume create ferrule-sync-data

docker run -d \
  --name ferrule-staff-sync \
  --restart unless-stopped \
  -p 127.0.0.1:8787:8787 \
  -v ferrule-sync-data:/data \
  -e STAFF_SYNC_TOKENS="$(openssl rand -hex 32)" \
  ferrule-staff-sync
```

### TLS Reverse Proxy (Caddy on a VPS)

Bind the container to `127.0.0.1:8787` and terminate HTTPS with Caddy. Setting `flush_interval -1` ensures Server-Sent Events (`GET /v1/events`) stream immediately without proxy buffering:

```caddyfile
sync.pigmentshow.us {
    reverse_proxy 127.0.0.1:8787 {
        flush_interval -1
    }
}
```

Verify TLS and health:

```bash
curl -fsS https://sync.pigmentshow.us/health
```

## Fly.io Deployment

1. Create the app and a 1 GB persistent volume (keep a single machine for SQLite):

```bash
fly launch --no-deploy --name pigment-staff-sync
fly volumes create staff_sync_data --region dfw --size 1
fly secrets set STAFF_SYNC_TOKENS="$(openssl rand -hex 32)"
```

2. Add a `fly.toml` in your deployment directory:

```toml
app = "pigment-staff-sync"
primary_region = "dfw"

[build]
  dockerfile = "Dockerfile"

[env]
  STAFF_SYNC_BIND = "0.0.0.0:8787"
  STAFF_SYNC_DB = "/data/staff-sync.db"

[mounts]
  source = "staff_sync_data"
  destination = "/data"

[http_service]
  internal_port = 8787
  force_https = true
  auto_stop_machines = "off"
  auto_start_machines = true
  min_machines_running = 1

  [[http_service.checks]]
    grace_period = "10s"
    interval = "30s"
    method = "GET"
    timeout = "5s"
    path = "/health"

[[vm]]
  size = "shared-cpu-1x"
  memory = "256mb"
```

3. Deploy:

```bash
fly deploy
```

> Note: Keep `auto_stop_machines = "off"` and `min_machines_running = 1` so long-lived SSE connections stay open and SQLite remains single-writer.

## Token Rotation and Backups

- **Zero-downtime token rotation**: Add the new token alongside the old token in `STAFF_SYNC_TOKENS="old-token,new-token"`, restart the server, update staff clients in **Settings -> Pigment Staff Sync**, then remove `old-token` and restart again. If a client's token is rejected (`401`), Ferrule preserves all queued offline edits in `staff_outbox` and surfaces an error badge until reconnected.
- **SQLite backups**: Because SQLite runs in WAL mode, take consistent snapshots using `sqlite3 /data/staff-sync.db ".backup '/backups/staff-sync.db'"` rather than copying the file while live writes occur.

## First-Rollout Checklist

Run this end-to-end checklist once before inviting the full Pigment staff team:

1. **Mint the production staff token**:
   - Generate a 32-byte random token (`openssl rand -hex 32`) and save it in your password manager.
   - Never commit the token to git or paste it into public channels.
2. **Deploy `staff-sync-server` with TLS**:
   - Set `STAFF_SYNC_TOKENS` and mount a persistent volume for `STAFF_SYNC_DB` (see Docker + Caddy or Fly.io above).
   - Confirm `curl -fsS https://sync.pigmentshow.us/health` returns `{"status":"ok"}`.
   - Confirm unauthenticated `GET /v1/snapshot` returns HTTP `401`, and authenticated `GET /v1/snapshot` with `Authorization: Bearer <token>` returns HTTP `200` with JSON.
   - Confirm `curl -N -H "Authorization: Bearer <token>" https://sync.pigmentshow.us/v1/events` stays open with `Content-Type: text/event-stream` (verifying your reverse proxy does not buffer SSE).
3. **Verify live Pigment IONOS login in Ferrule Desktop**:
   - Install the packaged Ferrule build from `release-artifacts/v<version>/`.
   - Add a Pigment role mailbox via **Add Account -> Pigment Staff (IONOS)** and confirm inbox sync and a test outbound email succeed.
   - Confirm the **From** address stays locked to the Pigment space and that credentials are stored in the OS keychain.
4. **Verify live Staff Sync SSE across two clients (or two profiles)**:
   - Open **Settings -> Pigment Staff Sync**, connect to `https://sync.pigmentshow.us` with the production token, and confirm the connection badge shows online.
   - Share a test thread from a Pigment space, add an internal comment or handoff status change, and confirm the second client receives the update in real time over SSE without manually refreshing.
5. **Verify offline outbox flush**:
   - Temporarily disconnect Wi-Fi (or stop `staff-sync-server`), post an internal comment or toggle an action item, and confirm the UI displays `Offline (1 pending)`.
   - Reconnect the network (or restart `staff-sync-server`) and confirm the pending counter flushes automatically to `0` and the edit appears on the second client.
6. **Verify SQLite persistence**:
   - Restart `staff-sync-server` and confirm `/v1/snapshot` still contains your test shared thread, comments, and action items.
