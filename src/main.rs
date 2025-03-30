#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> Result<(), eyre::Report> {
    use axum::{
        extract::{self, Extension},
        routing::post,
        Router, ServiceExt,
    };
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use std::{collections::HashMap, sync::Arc};
    use stream_alerts::fileserv::file_and_error_handler;
    use stream_alerts::{
        alerts::{Alert, AlertId, AlertMessage},
        app::*,
    };
    use tokio::sync::{broadcast, RwLock};
    use tower_http::trace::{DefaultMakeSpan, MakeSpan, TraceLayer};
    use tracing::instrument;

    async fn leptos_handler(
        auth: stream_alerts::auth::AuthSession,
        Extension(manager): Extension<stream_alerts::alerts::AlertManager>,
        //user: Option<Extension<stream_alerts::auth::User>>,
        req: axum::http::Request<axum::body::Body>,
    ) -> axum::response::Response {
        use axum::response::IntoResponse;

        let span = tracing::info_span!("leptos", auth.current_user = ?auth.user);
        let span_c = span.clone();
        let span_c2 = span.clone();
        let handler = leptos_axum::render_app_async_with_context(
            move || {
                let _s = span_c.enter();
                tracing::info!("providing context");
                //provide_context( auth_session.clone());
                if auth.user.is_some() {
                    provide_context::<stream_alerts::alerts::AlertManager>(manager.clone());
                }
                provide_context::<stream_alerts::auth::AuthSession>(auth.clone());
            },
            move || {
                let _s = span_c2.enter();
                view! { <App/> }
            },
        );
        tracing::Instrument::instrument(handler(req), span)
            .await
            .into_response()
    }

    stream_alerts::util::install_utils().unwrap();
    let opts = <stream_alerts::opts::Opts as clap::Parser>::parse();
    // Setting get_configuration(None) means we'll be using cargo-leptos's env values
    // For deployment these variables are:
    // <https://github.com/leptos-rs/start-axum#executing-a-server-on-a-remote-machine-without-the-toolchain>
    // Alternately a file can be specified such as Some("Cargo.toml")
    // The file would need to be included with the executable when moved to deployment
    let conf = get_configuration(None).unwrap();
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let routes = generate_route_list(App);

    let (alert_router, manager) = stream_alerts::alerts::setup(&opts).await?;

    let auth_layer = stream_alerts::auth::setup(&opts).await?;
    #[derive(Clone, axum::extract::FromRef)]
    pub struct AppState {
        pub leptos_options: LeptosOptions,
        pub alert_manager: new::AlertManager,
    }

    let app_state = AppState {
        leptos_options: leptos_options.clone(),
        alert_manager: manager.clone(),
    };

    // build our application with a route
    let app: Router<_> = Router::new()
        .nest("/alert", alert_router)
        .route(
            "/backend/*fn_name",
            post(
                move |
                      Extension(manager): Extension<stream_alerts::alerts::AlertManager>,
                      Extension(user_store): Extension<stream_alerts::auth::Users>,
                      auth: stream_alerts::auth::AuthSession,
                      path: axum::extract::Path<String>,
                      req| {
                    use axum::response::IntoResponse;

                    let span =
                        tracing::info_span!("server_fn", server_fn = path.0, auth.user = ?auth.user, ?auth);
                    tracing::Instrument::instrument(
                        async move {
                            if auth.user.is_none() && !path.0.contains("public/") {
                                tracing::warn!("Unauthorized access");
                                return (http::StatusCode::UNAUTHORIZED, "Unauthorized")
                                    .into_response();
                            }
                            if auth.user.is_some() {
                                tracing::debug!("authorized access");
                            } else {
                                tracing::debug!("public access");
                            }
                            let path = axum::extract::Path(
                                path.0.trim_start_matches("public/").to_owned(),
                            );
                            leptos_axum::handle_server_fns_with_context(
                                move || {
                                    provide_context::<stream_alerts::alerts::AlertManager>(
                                        manager.clone(),
                                    );
                                    provide_context::<stream_alerts::auth::AuthSession>(
                                        auth.clone(),
                                    );
                                    provide_context::<stream_alerts::auth::Users>(
                                        user_store.clone(),
                                    );
                                },
                                req,
                            )
                            .await
                            .into_response()
                        },
                        span,
                    )
                },
            ),
        )
        .leptos_routes_with_handler(routes, axum::routing::get(leptos_handler))
        .fallback(file_and_error_handler)
        .layer(Extension(manager.clone()))
        .layer(auth_layer)
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
                    // get cookie `sid`
                    let sid = request.headers().get("cookie").and_then(|cookie| {
                        let cookie = cookie.to_str().ok()?;
                        cookie
                        .split("; ")
                    .filter_map(|cookie| {
                        cookie.strip_prefix("stream_alerts_session=")
                    })
                    .next()
                    });
                    tracing::info_span!(
                        "http-request",
                        sid = sid,
                        ip = tracing::field::Empty,
                        status_code = tracing::field::Empty,
                        uri = tracing::field::display(request.uri()),
                        method = tracing::field::display(request.method()),
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
                            tracing::trace!("error response generated");
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
            )
            .with_state(app_state.clone());

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    tracing::info!("listening on http://{}", &addr);
    if addr.ip().is_loopback() && addr.port() != 80 {
        tracing::info!("available on http://localhost:{}", addr.port())
    }
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app.into_make_service())
        .await
        .map_err(Into::into)
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    // no client-side main function
    // unless we want this to work with e.g., Trunk for a purely client-side app
    // see lib.rs for hydration function instead
}
