use cfg_if::cfg_if;
use leptos::error::Errors;

cfg_if! { if #[cfg(feature = "ssr")] {
    use axum::{
        body::{Body},
        extract::{self, Extension},
        response::IntoResponse,
        http::{Request, Response, StatusCode, Uri},
    };
    use axum::response::Response as AxumResponse;
    use tower::ServiceExt;
    use tower_http::services::ServeDir;
    use std::sync::Arc;
    use leptos::prelude::*;
    use crate::error_template::{ErrorTemplate};
    use crate::error_template::AppError;

    pub async fn file_and_error_handler(uri: Uri, extract::State(options): extract::State<LeptosOptions>, req: Request<Body>) -> AxumResponse {
        let root = options.site_root.clone();
        let res = get_static_file(uri.clone(), &root, req.headers()).await.unwrap();

        if res.status() == StatusCode::OK {
           res.into_response()
        } else {
            let mut errors = Errors::default();
            errors.insert_with_default_key(AppError::NotFound(uri.clone()));
            let handler = leptos_axum::render_app_to_stream(move || view!{<ErrorTemplate outside_errors=errors.clone()/>});
            handler(req).await.into_response()
        }
    }
    async fn get_static_file(
        uri: Uri,
        root: &str,
        headers: &http::HeaderMap<http::HeaderValue>,
    ) -> Result<Response<Body>, (StatusCode, String)> {
        use axum::http::header::ACCEPT_ENCODING;

        let req = Request::builder().uri(uri);

        let req = match headers.get(ACCEPT_ENCODING) {
            Some(value) => req.header(ACCEPT_ENCODING, value),
            None => req,
        };

        let req = req.body(Body::empty()).unwrap();
        // `ServeDir` implements `tower::Service` so we can call it with `tower::ServiceExt::oneshot`
        // This path is relative to the cargo root
        match ServeDir::new(root)
            .precompressed_gzip()
            .precompressed_br()
            .oneshot(req)
            .await
        {
            Ok(res) => Ok(res.into_response()),
            Err(err) => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Something went wrong: {err}"),
            )),
        }
    }
}}
