# syntax = docker/dockerfile:1.2
FROM rust:1-alpine3.15 as builder
WORKDIR /app
ARG BUILD_DEPS
RUN apk add --no-cache ${BUILD_DEPS}
COPY . .
ARG RUSTFLAGS=-Ctarget-feature=-crt-static
RUN --mount=type=cache,target=$CARGO_HOME/git \
    --mount=type=cache,target=$CARGO_HOME/registry \
    --mount=type=cache,sharing=private,target=/app/target \
    cargo -V; cargo build --release --bin is_sessis_live && mv /app/target/release/is_sessis_live /app/is_sessis_live
FROM alpine:3.15 as runtime
WORKDIR /app
ARG RUN_DEPS
RUN apk add --no-cache \
        ${RUN_DEPS}
COPY --from=builder /app/is_sessis_live /app/is_sessis_live
ENTRYPOINT "/app/is_sessis_live"