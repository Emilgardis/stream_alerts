#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> Result<(), eyre::Report> {
    use axum::{extract::Extension, routing::post, Router};
    use axum_login::AuthLayer;
    use leptos::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use std::sync::Arc;
    use stream_alerts::app::*;
    use stream_alerts::fileserv::file_and_error_handler;
    use tokio::sync::RwLock;
    use tower_http::trace::{DefaultMakeSpan, MakeSpan, TraceLayer};

    #[axum::debug_handler]
    async fn leptos_handler(
        auth_context: stream_alerts::auth::AuthContext,
        Extension(manager): Extension<stream_alerts::alerts::AlertManager>,
        Extension(options): Extension<Arc<LeptosOptions>>,
        user: Option<Extension<stream_alerts::auth::User>>,
        req: http::Request<axum::body::Body>,
    ) -> axum::response::Response {
        use axum::response::IntoResponse;
        tracing::info!(?user, "got req");
        let handler = leptos_axum::render_app_to_stream_with_context(
            (*options).clone(),
            move |cx| {
                //provide_context(cx, auth_session.clone());
                provide_context::<stream_alerts::alerts::AlertManager>(cx, manager.clone());
                provide_context::<stream_alerts::auth::AuthContext>(cx, auth_context.clone());
            },
            move |cx| {
                view! { cx, <App/> }
            },
        );
        handler(req).await.into_response()
    }

    stream_alerts::util::install_utils().unwrap();
    let opts = <stream_alerts::opts::Opts as clap::Parser>::parse();
    // Setting get_configuration(None) means we'll be using cargo-leptos's env values
    // For deployment these variables are:
    // <https://github.com/leptos-rs/start-axum#executing-a-server-on-a-remote-machine-without-the-toolchain>
    // Alternately a file can be specified such as Some("Cargo.toml")
    // The file would need to be included with the executable when moved to deployment
    let conf = get_configuration(None).await.unwrap();
    let leptos_options = Arc::new(conf.leptos_options);
    let addr = leptos_options.site_addr;
    let routes = generate_route_list(|cx| view! { cx, <App/> }).await;

    // Register serverfns
    stream_alerts::app::register_server_fns();

    let (alert_router, manager) = stream_alerts::alerts::setup(&opts).await?;

    let user_store = Arc::new(RwLock::new(std::collections::HashMap::<
        i64,
        stream_alerts::auth::User,
    >::new()));

    let session_store = axum_login::axum_sessions::async_session::MemoryStore::new();
    let session_layer = axum_login::axum_sessions::SessionLayer::new(session_store, &[0; 64]);
    let user_store = axum_login::memory_store::MemoryStore::new(&user_store);
    let auth_layer = axum_login::AuthLayer::new(user_store, &[0; 64]);
    // build our application with a route
    let app: Router<_> = Router::new()
        .nest("/alert", alert_router)
        .route(
            "/backend/*fn_name",
            post(
                move |user: Option<Extension<stream_alerts::auth::User>>,
                      Extension(manager): Extension<stream_alerts::alerts::AlertManager>,
                      path: axum::extract::Path<String>,
                      header,
                      query,
                      req| {
                    use axum::response::IntoResponse;

                    async move {
                        tracing::info!("got req for {}", path.0);
                        if !path.0.contains("unauthed/") && user.is_none() {
                            return (http::StatusCode::UNAUTHORIZED, "Unauthorized")
                                .into_response();
                        }
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
                        .await
                        .into_response()
                    }
                },
            ),
        )
        .leptos_routes_with_handler(routes, axum::routing::get(leptos_handler))
        .fallback(file_and_error_handler)
        // TODO: Use state
        .layer(Extension(manager.clone()))
        .layer(Extension(Arc::clone(&leptos_options)))
        .layer(auth_layer)
        .layer(session_layer)
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
