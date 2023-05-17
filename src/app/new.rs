use gloo_net::http::Method;
use gloo_utils::format::JsValueSerdeExt;
use leptos::*;
use leptos_meta::*;
use leptos_router::*;

#[component]
#[track_caller]
pub fn NewAlert(cx: Scope) -> impl IntoView {
    tracing::info!(?cx);
}
