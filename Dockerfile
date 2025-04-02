# syntax = docker/dockerfile:1.2
FROM rust:1-slim-bullseye as builder
WORKDIR /app
ARG BUILD_DEPS
RUN apt-get update && apt-get install -y ${BUILD_DEPS}
ARG CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
#RUN wget https://github.com/cargo-bins/cargo-binstall/releases/latest/download/cargo-binstall-x86_64-unknown-linux-musl.tgz -O /tmp/cargo-binstall.tgz && \
#    tar -xvf /tmp/cargo-binstall.tgz -C /usr/local/cargo/bin && \
#    rm /tmp/cargo-binstall.tgz
RUN rustup target add wasm32-unknown-unknown
RUN --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cargo install cargo-leptos
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo -V; cargo leptos -V; cargo leptos build --release && cp -r /app/target/release/ /app/ && cp -r /app/target/site /app/
FROM debian:bullseye-slim as runtime
WORKDIR /app
ARG RUN_DEPS
RUN apt-get update && apt-get install -y ${RUN_DEPS}
COPY --from=builder /app/release/stream_alerts /app/stream_alerts
COPY --from=builder /app/site /app/site
ENV LEPTOS_SITE_ADDR="0.0.0.0:3000" \
    APP_ENVIRONMENT="production" \
    LEPTOS_SITE_ROOT="site"
EXPOSE 3000
ENTRYPOINT ["/app/stream_alerts"]