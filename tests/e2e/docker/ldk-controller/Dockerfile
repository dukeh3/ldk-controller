# syntax=docker/dockerfile:1

# Pre-built binary approach: the host builds ldk-controller and provides
# the binary via the build context, avoiding path dependency issues.
# Build with:  docker build -f ... --build-arg BINARY=target/debug/ldk-controller .

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --create-home --home-dir /var/lib/ldk-controller ldk-controller
WORKDIR /var/lib/ldk-controller

ARG BINARY=target/debug/ldk-controller
COPY ${BINARY} /usr/local/bin/ldk-controller

USER ldk-controller
ENTRYPOINT ["/usr/local/bin/ldk-controller"]
