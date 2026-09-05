# Both binaries, from one build.
#
# The API and the worker share nearly every dependency, so building them
# separately would compile the workspace twice for no benefit. Which one runs is
# decided by the compose service, not by the image.
#
# Build from the repository root:
#   docker build -f docker/Dockerfile -t anthovai:dev .

# ---- planner ---------------------------------------------------------------
#
# Manifests first, then a throwaway build of the dependency graph. Docker caches
# that layer, so editing our own code recompiles our own crates and nothing
# else — the difference between a thirty-second rebuild and a ten-minute one.

FROM rust:1-bookworm AS planner
WORKDIR /build

RUN cargo install cargo-chef --locked --version ^0.1

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates crates
COPY apps apps
RUN cargo chef prepare --recipe-path recipe.json

# ---- builder ---------------------------------------------------------------

FROM rust:1-bookworm AS builder
WORKDIR /build

RUN cargo install cargo-chef --locked --version ^0.1

COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates crates
COPY apps apps
COPY migrations migrations

RUN cargo build --release --bin anthovai-api --bin anthovai-worker

# ---- runtime ---------------------------------------------------------------
#
# Debian slim rather than distroless or Alpine: `rustls` needs the system trust
# store to verify a provider's certificate, and musl would mean rebuilding the
# whole tree against a different libc for no gain here.

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# Nothing here needs root, and a container that runs as root is one container
# escape away from a host compromise.
RUN useradd --system --uid 10001 --create-home --home-dir /app anthovai
WORKDIR /app

COPY --from=builder /build/target/release/anthovai-api /usr/local/bin/
COPY --from=builder /build/target/release/anthovai-worker /usr/local/bin/

# Read at startup by both binaries. Mounted over in a real deployment, which is
# why the models file is a separate concern from the code.
COPY config config
COPY migrations migrations

USER anthovai
EXPOSE 8080

# Liveness only. Readiness — which reaches the database, storage and the model
# providers — belongs to the orchestrator, not to a container's own opinion of
# itself: a database outage should not restart every API container at once.
HEALTHCHECK --interval=15s --timeout=3s --start-period=20s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/internal/health || exit 1

CMD ["anthovai-api"]
