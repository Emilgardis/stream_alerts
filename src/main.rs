#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> Result<(), eyre::Report> {
    use axum::{
        extract::{self, Extension},
        routing::post,
        Router,
    };
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};

    use stream_alerts::fileserv::file_and_error_handler;
    use stream_alerts::app::*;

    use tower_http::trace::{DefaultMakeSpan, MakeSpan, TraceLayer};


    async fn leptos_handler(
        auth: stream_alerts::auth::AuthSession,
        Extension(manager): Extension<stream_alerts::alerts::AlertManager>,
        extract::State(appstate): extract::State<AppState>,
        //user: Option<Extension<stream_alerts::auth::User>>,
        req: axum::http::Request<axum::body::Body>,
    ) -> axum::response::Response {
        use axum::response::IntoResponse;

        let span = tracing::info_span!("leptos", auth.current_user = ?auth.user);
        let span_c = span.clone();
        let span_c2 = span.clone();
        let options = appstate.leptos_options.clone();
        let appstate = appstate.clone();
        let handler = leptos_axum::render_app_async_with_context(
            move || {
                let _s = span_c.enter();
                tracing::trace!("providing context");
                //provide_context( auth_session.clone());
                provide_context::<stream_alerts::alerts::AlertManager>(manager.clone());
                provide_context::<stream_alerts::auth::AuthSession>(auth.clone());
                provide_context::<AppState>(appstate.clone());
            },
            move || {
                let options = options.clone();
                let _s = span_c2.enter();
                view! { <!DOCTYPE html>
                <html lang="en">
                    <head>
                        <meta charset="utf-8"/>
                        <meta name="viewport" content="width=device-width, initial-scale=1"/>
                        <AutoReload options=options.clone() />
                        <HydrationScripts options/>
                        <leptos_meta::MetaTags/>
                    </head>
                    <body>
                        <App/>
                    </body>
                </html> }
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

    let app_state = AppState {
        leptos_options: leptos_options.clone(),
        alert_manager: manager.clone(),
    };

    let app_state2 = app_state.clone();

    // build our application with a route
    let app: Router<_> = Router::new()
        .nest("/alert", alert_router)
        .route(
            "/backend/*fn_name",
            post(
                move |
                      Extension(manager): Extension<stream_alerts::alerts::AlertManager>,
                      auth: stream_alerts::auth::AuthSession,
                      path: axum::extract::Path<String>,
                      req| {
                    use axum::response::IntoResponse;
                    let app_state = app_state2.clone();
                    let span =
                        tracing::info_span!("server_fn", server_fn = path.0, auth.user = ?auth.user.as_ref().map(|u| &u.name));
                    tracing::Instrument::instrument(
                        async move {
                            if auth.user.is_some() {
                                tracing::debug!("authorized access");
                            } else {
                                tracing::debug!("public access");
                            }
                            leptos_axum::handle_server_fns_with_context(
                                move || {
                                    provide_context::<stream_alerts::alerts::AlertManager>(
                                        manager.clone(),
                                    );
                                    provide_context::<stream_alerts::auth::AuthSession>(
                                        auth.clone(),
                                    );
                                    provide_context::<stream_alerts::app::AppState>(
                                        app_state.clone(),
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
