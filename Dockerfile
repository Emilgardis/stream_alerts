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
    cargo -V; cargo build --release --bin {{crate_name}} && mv /app/target/release/{{crate_name}} /app/{{crate_name}}
FROM alpine:3.15 as runtime
WORKDIR /app
ARG RUN_DEPS
RUN apk add --no-cache \
        ${RUN_DEPS}
COPY --from=builder /app/{{crate_name}} /app/{{crate_name}}
ENTRYPOINT "/app/{{crate_name}}"