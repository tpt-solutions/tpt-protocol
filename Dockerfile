# Daemon image. Builds the daemon only (the UI crates are not needed in a container).
#
# Build:   docker build -t tpt-protocol-daemon .
# Run:     docker run --rm -p 127.0.0.1:9080:9080 tpt-protocol-daemon
#
# The daemon binds 127.0.0.1 by default, which is unreachable from the host
# through a published port. The image binds 0.0.0.0 inside the container, and the
# host-side -p 127.0.0.1:... keeps the port on loopback. Add --token to the
# command when the port is exposed more widely.
#
# Tested: built and run on Docker Desktop 29.9.2 (2026-11-10); /healthz answers on the host port.

FROM rust:1.90-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p tpt-protocol-daemon

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 tpt
COPY --from=build /src/target/release/tpt-protocol-daemon /usr/local/bin/tpt-protocol-daemon
USER tpt
EXPOSE 9080
ENTRYPOINT ["tpt-protocol-daemon"]
CMD ["--simulate", "modbus", "--bind", "0.0.0.0"]
