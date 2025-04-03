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
RUN curl --proto '=https' --tlsv1.2 -LsSf https://github.com/leptos-rs/cargo-leptos/releases/download/v0.2.32/cargo-leptos-installer.sh | sh
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cargo -V; cargo leptos -V; cargo leptos build --release -vv && cp -r /app/target/release/ /app/ && cp -r /app/target/site /app/
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