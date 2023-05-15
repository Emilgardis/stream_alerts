#![warn(clippy::unwrap_in_result)]
#![warn(clippy::todo)]
pub mod alerts;
mod ip;
pub mod opts;
pub mod util;

use axum::{
    extract::Extension,
    routing::{get, get_service},
    Router,
};
use clap::Parser;
use eyre::Context;
use hyper::StatusCode;
use opts::Opts;
use std::{collections::HashMap, sync::Arc};
use tokio::{
    sync::{broadcast, RwLock},
    task::JoinHandle,
};
use tower_http::{
    catch_panic::CatchPanicLayer,
    services::ServeDir,
    trace::{DefaultMakeSpan, MakeSpan, TraceLayer},
};

use alerts::{Alert, AlertId};

#[tokio::main]
async fn main() -> Result<(), eyre::Report> {
    util::install_utils()?;
    let opts = Opts::parse();

    tracing::debug!(
        "App started!\n{}",
        Opts::try_parse_from(["app", "--version"])
            .unwrap_err()
            .to_string()
    );

    run(&opts)
        .await
        .with_context(|| "when running application")?;

    Ok(())
}

pub async fn run(opts: &Opts) -> eyre::Result<()> {
    let app = Router::new()
        .nest_service(
            "/static",
            get_service(ServeDir::new("./static/")).handle_error(|error| async move {
                tracing::error!("{}", error);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Unhandled internal error".to_string(),
                )
            }),
        )
        .nest("/alert", alerts::setup(opts).await?)
        .layer(
            tower::ServiceBuilder::new()
                //.layer(axum::error_handling::HandleErrorLayer::new(handle_error))
                .layer(Extension(Arc::new(opts.clone())))
                .layer(
                    TraceLayer::new_for_http()
                        .on_failure(|error, _latency, _span: &tracing::Span| {
                            tracing::error!(error=%error);
                        })
                        .make_span_with(|request: &axum::http::Request<axum::body::Body>| {
                            DefaultMakeSpan::new()
                                //.include_headers(true)
                                .make_span(request)
                                .in_scope(|| {
                                    tracing::info_span!(
                                        "http-request",
                                        status_code = tracing::field::Empty,
                                        uri = tracing::field::display(request.uri()),
                                        method = tracing::field::display(request.method()),
                                        ip = tracing::field::Empty,
                                    )
                                })
                        })
                        .on_response(
                            |response: &axum::http::Response<_>,
                             _latency: std::time::Duration,
                             span: &tracing::Span| {
                                span.record(
                                    "status_code",
                                    &tracing::field::display(response.status()),
                                );

                                tracing::info!("response generated");
                            },
                        )
                        .on_request(
                            |request: &axum::http::Request<axum::body::Body>,
                             span: &tracing::Span| {
                                if let Some(ip) =
                                    ip::real_ip(request.headers(), request.extensions())
                                {
                                    span.record("ip", tracing::field::display(ip));
                                } else {
                                    span.record("ip", "<unknown>");
                                }
                                tracing::debug!("request received");
                            },
                        ),
                )
                .layer(CatchPanicLayer::new()),
        );

    let address = (opts.interface, opts.port).into();
    let server = tokio::spawn(async move {
        axum::Server::bind(&address)
            .serve(app.into_make_service_with_connect_info::<std::net::SocketAddr>())
            .await
            .wrap_err_with(|| "when serving")?;
        Ok::<(), eyre::Report>(())
    });
    tracing::info!("spinning up server! http://{}", address);
    let r = tokio::try_join!(flatten(server),);
    r?;
    Ok(())
}

async fn flatten<T>(handle: JoinHandle<Result<T, eyre::Report>>) -> Result<T, eyre::Report> {
    match handle.await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(err)) => Err(err),
        Err(e) => Err(e).wrap_err_with(|| "handling failed"),
    }
}
