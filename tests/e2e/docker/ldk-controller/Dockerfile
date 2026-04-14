# syntax=docker/dockerfile:1

FROM rust:1-bookworm AS builder
WORKDIR /app

# HACK: copy bip321 path dependency into /bip321 so that
# Cargo.toml's `path = "../bip321"` resolves from /app.
# Requires: --build-context bip321=../bip321
# TODO: replace with git/registry dep (see GitHub issue)
COPY --from=bip321 . /bip321/

COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --bin ldk-controller

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --create-home --home-dir /var/lib/ldk-controller ldk-controller
WORKDIR /var/lib/ldk-controller

COPY --from=builder /app/target/release/ldk-controller /usr/local/bin/ldk-controller

USER ldk-controller
ENTRYPOINT ["/usr/local/bin/ldk-controller"]
