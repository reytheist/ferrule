# Multi-stage build for the standalone Pigment Staff Sync companion server.
# Build from the repository root:
#   docker build -t ferrule-staff-sync .

FROM rust:1-bookworm AS builder

WORKDIR /src

# Copy workspace manifests first for dependency layer caching.
COPY Cargo.toml Cargo.lock ./
COPY crates/mail-core/Cargo.toml crates/mail-core/Cargo.toml
COPY crates/staff-sync-server/Cargo.toml crates/staff-sync-server/Cargo.toml
COPY src-tauri/Cargo.toml src-tauri/Cargo.toml

# Copy crate sources needed for staff-sync-server (depends on mail-core).
COPY crates/mail-core crates/mail-core
COPY crates/staff-sync-server crates/staff-sync-server

# Create a minimal stub for src-tauri so Cargo workspace resolution succeeds
# without pulling GUI/WebView system libraries into the server build image.
RUN mkdir -p src-tauri/src && \
    printf 'pub fn run() {}\n' > src-tauri/src/lib.rs && \
    printf 'fn main() {}\n' > src-tauri/src/main.rs

RUN cargo build --release --locked -p staff-sync-server --bin staff-sync-server

FROM debian:bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/* && \
    useradd --system --uid 10001 --create-home --home-dir /data ferrule

COPY --from=builder /src/target/release/staff-sync-server /usr/local/bin/staff-sync-server

WORKDIR /data
VOLUME ["/data"]

# Bind to all interfaces inside the container for reverse-proxy networking.
# In production, always place behind a TLS-terminating reverse proxy and/or publish
# only on loopback (e.g. `docker run -p 127.0.0.1:8787:8787 ...`).
ENV STAFF_SYNC_BIND=0.0.0.0:8787
ENV STAFF_SYNC_DB=/data/staff-sync.db

USER ferrule
EXPOSE 8787

ENTRYPOINT ["/usr/local/bin/staff-sync-server"]
