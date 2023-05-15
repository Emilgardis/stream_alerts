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
    let (sender, _) = broadcast::channel(16);
    let retainer = Arc::new(retainer::Cache::<axum::http::HeaderValue, ()>::new());
    let ret = retainer.clone();
    let retainer_cleanup = async move {
        ret.monitor(10, 0.50, tokio::time::Duration::from_secs(86400 / 2))
            .await;
        Ok::<(), eyre::Report>(())
    };
    let map = Arc::new(RwLock::new(HashMap::<AlertId, Alert>::new()));
    read_alerts(&map, opts.db_path.clone()).await?;
    let app = Router::new()
        .route(
            "/ws/:id",
            get({
                let sender = sender.clone();
                move |ws, id, map| alerts::handler(ws, sender, id, map)
            }),
        )
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
        .nest("/alert", alerts::route( sender.clone()))
        .layer(
            tower::ServiceBuilder::new()
                //.layer(axum::error_handling::HandleErrorLayer::new(handle_error))
                .layer(Extension(Arc::new(sender.clone())))
                .layer(Extension(retainer.clone()))
                .layer(Extension(map.clone()))
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
    let r = tokio::try_join!(flatten(server), flatten(tokio::spawn(retainer_cleanup)),);
    r?;
    Ok(())
}

async fn read_alerts(
    map: &RwLock<HashMap<AlertId, Alert>>,
    db_path: std::path::PathBuf,
) -> Result<(), eyre::Report> {
    let mut i = tokio::fs::read_dir(db_path).await?;
    let mut map = map.write().await;
    while let Some(entry) = i.next_entry().await? {
        if entry.file_type().await?.is_file() {
            let path = entry.path();
            let alert = Alert::load_alert(path).await?;

            map.insert(alert.alert_id.clone(), alert);
        }
    }
    Ok(())
}

async fn flatten<T>(handle: JoinHandle<Result<T, eyre::Report>>) -> Result<T, eyre::Report> {
    match handle.await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(err)) => Err(err),
        Err(e) => Err(e).wrap_err_with(|| "handling failed"),
    }
}
