FROM rust:1.97.1-bookworm AS builder

# The context is `rust/`, not `rust/backend/`, because the workspace patches
# librqbit to `../vendor/librqbit` — a build that cannot see it fails while
# resolving dependencies, long before it reaches any code the server uses.
WORKDIR /workspace
COPY . .
WORKDIR /workspace/backend
RUN cargo build --locked --release -p portalis-nexus-server

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home nexus

COPY --from=builder /workspace/backend/target/release/portalis-nexus-server /usr/local/bin/portalis-nexus-server

USER 10001:10001
ENV PORTALIS_NEXUS_LISTEN_ADDR=0.0.0.0:8080
# Both, and for the same reason the compose file publishes both: the service
# speaks QUIC over UDP, while health and orchestration are TCP.
EXPOSE 8080/udp
EXPOSE 8080/tcp

ENTRYPOINT ["/usr/local/bin/portalis-nexus-server"]
