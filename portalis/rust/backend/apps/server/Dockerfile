# Build from rust/backend:
# docker build -f apps/server/Dockerfile -t portalis-nexus .
FROM rust:1.85-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --package portalis-nexus-server

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --create-home --home-dir /nonexistent portalis
COPY --from=build /src/target/release/portalis-nexus-server /usr/local/bin/portalis-nexus-server
USER portalis
ENV PORTALIS_NEXUS_LISTEN_ADDR=0.0.0.0:8080
ENV PORTALIS_NEXUS_DATA_DIR=/var/lib/portalis-nexus
VOLUME ["/var/lib/portalis-nexus"]
EXPOSE 8080/tcp
EXPOSE 8080/udp
ENTRYPOINT ["/usr/local/bin/portalis-nexus-server"]
