#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> Result<(), eyre::Report> {
    use axum::{extract::Extension, routing::post, Router};
    use leptos::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use std::sync::Arc;
    use stream_alerts::app::*;
    use stream_alerts::fileserv::file_and_error_handler;
    use tower_http::trace::{DefaultMakeSpan, MakeSpan, TraceLayer};
    stream_alerts::util::install_utils().unwrap();
    let opts = <stream_alerts::opts::Opts as clap::Parser>::parse();
    // Setting get_configuration(None) means we'll be using cargo-leptos's env values
    // For deployment these variables are:
    // <https://github.com/leptos-rs/start-axum#executing-a-server-on-a-remote-machine-without-the-toolchain>
    // Alternately a file can be specified such as Some("Cargo.toml")
    // The file would need to be included with the executable when moved to deployment
    let conf = get_configuration(None).await.unwrap();
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let routes = generate_route_list(|cx| view! { cx, <App/> }).await;

    // Register serverfns
    stream_alerts::app::register_server_fns();

    let (alert_router, manager) = stream_alerts::alerts::setup(&opts).await?;

    let manager_fn = manager.clone();
    // build our application with a route
    let app = Router::new()
        .route(
            "/backend/*fn_name",
            post(move |path, header, query, req| {
                let manager = manager.clone();
                leptos_axum::handle_server_fns_with_context(
                    path,
                    header,
                    query,
                    move |cx| {
                        leptos::provide_context::<stream_alerts::alerts::AlertManager>(
                            cx,
                            manager.clone(),
                        );
                    },
                    req,
                )
            }),
        )
        .nest("/alert", alert_router)
        .leptos_routes_with_context(
            leptos_options.clone(),
            routes,
            move |cx| {
                leptos::provide_context::<stream_alerts::alerts::AlertManager>(
                    cx,
                    manager_fn.clone(),
                );
            },
            move |cx| {
                view! { cx, <App/> }
            },
        )
        .fallback(file_and_error_handler)
        .layer(Extension(Arc::new(leptos_options)))
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
                                user_agent = request
                                    .headers()
                                    .get(http::header::USER_AGENT)
                                    .map(tracing::field::debug)
                            )
                        })
                })
                .on_response(
                    |response: &axum::http::Response<_>,
                     _latency: std::time::Duration,
                     span: &tracing::Span| {
                        span.record("status_code", tracing::field::display(response.status()));
                        if response.status().is_success() {
                            tracing::trace!("response generated");
                        } else {
                            tracing::error!("error response generated");
                        }
                    },
                )
                .on_request(
                    |request: &axum::http::Request<axum::body::Body>, span: &tracing::Span| {
                        if let Some(ip) =
                            stream_alerts::ip::real_ip(request.headers(), request.extensions())
                        {
                            span.record("ip", tracing::field::display(ip));
                        } else {
                            span.record("ip", "<unknown>");
                        }
                        tracing::trace!("request received");
                    },
                ),
        );

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    log!("listening on http://{}", &addr);
    if addr.ip().is_loopback() && addr.port() != 80 {
        log!("available on http://localhost:{}", addr.port())
    }
    axum::Server::bind(&addr)
        .serve(app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .await
        .map_err(Into::into)
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    // no client-side main function
    // unless we want this to work with e.g., Trunk for a purely client-side app
    // see lib.rs for hydration function instead
}
