# Deploying `staff-sync-server`

`crates/staff-sync-server` is a lightweight, standalone HTTP + SSE + SQLite companion server for Ferrule's Pigment Staff Sync. It has zero external database or SaaS dependencies and runs comfortably in 64-128 MB RAM.

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
sync.pigmentminiatures.com {
    reverse_proxy 127.0.0.1:8787 {
        flush_interval -1
    }
}
```

Verify TLS and health:

```bash
curl -fsS https://sync.pigmentminiatures.com/health
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
