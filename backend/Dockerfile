# ── Stage 1: Build ────────────────────────────────────────────────────────────
# Use the full rust image (not slim) — it includes gcc and all C build tools
# needed by ring (rustls / sqlx dependency) without extra apt installs.
# Use the latest stable Rust to match whatever version generated Cargo.lock on the host.
# If you need a pinned version, check `rustc --version` on the host and use that tag.
FROM rust:latest AS builder

WORKDIR /app

# ── Dependency-caching layer ──────────────────────────────────────────────────
# Copy only the manifests first so this layer is only invalidated when
# dependencies change, not on every source edit.
COPY Cargo.toml Cargo.lock ./

# Create a minimal stub so cargo can compile all dependencies.
# We also need an empty migrations dir so the sqlx::migrate! macro
# (which reads files at compile time) doesn't error.
RUN mkdir -p src migrations && \
    echo 'fn main() {}' > src/main.rs

RUN cargo build --release

# Delete only the compiled artifacts for OUR binary so cargo knows
# to recompile main.rs in the next step, but keeps all dep artifacts cached.
RUN rm -f \
    target/release/inventory-service \
    target/release/deps/inventory_service-* \
    target/release/deps/libinventory_service-*

# ── Real build ────────────────────────────────────────────────────────────────
COPY src        ./src
COPY migrations ./migrations

# `touch` forces cargo to see main.rs as modified → only our crate recompiles.
RUN touch src/main.rs && cargo build --release

# ── Stage 2: Minimal runtime image ───────────────────────────────────────────
FROM debian:bookworm-slim

WORKDIR /app

# Only runtime libraries needed (no build tools).
RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/inventory-service .
COPY --from=builder /app/migrations ./migrations

RUN mkdir -p reports

EXPOSE 3000

CMD ["./inventory-service"]
