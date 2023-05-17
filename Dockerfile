# syntax = docker/dockerfile:1.2
FROM rust:1-alpine3.17 as builder
WORKDIR /app
ARG BUILD_DEPS
RUN apk add --no-cache ${BUILD_DEPS}
COPY . .
ARG RUSTFLAGS=-Ctarget-feature=-crt-static
RUN --mount=type=cache,target=$CARGO_HOME/git \
    --mount=type=cache,target=$CARGO_HOME/registry \
    --mount=type=cache,sharing=private,target=/app/target \
    cargo -V; cargo build --release --bin stream_alerts && mv /app/target/release/stream_alerts /app/stream_alerts
FROM alpine:3.17 as runtime
WORKDIR /app
ARG RUN_DEPS
RUN apk add --no-cache \
        ${RUN_DEPS}
COPY --from=builder /app/stream_alerts /app/stream_alerts
COPY ./static ./static
ENTRYPOINT ["/app/stream_alerts", "--interface", "0.0.0.0"]