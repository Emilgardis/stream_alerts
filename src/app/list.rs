use std::collections::BTreeMap;

use leptos::*;
use leptos_meta::*;
use leptos_router::*;

pub use super::login::*;
pub use crate::alerts::*;

#[component]
#[track_caller]
pub fn ListAlerts(cx: Scope) -> impl IntoView {
    let params = use_params_map(cx);

    let alerts = create_blocking_resource(
        cx,
        move || (),
        move |_| async move { crate::alerts::read_all_alerts(cx).await },
    );

    view! {cx,
        <p>"Alerts"</p>
        <Suspense fallback=move || view!{cx, <p>"loading"</p>}>
        <ErrorBoundary fallback=move |cx, e| {
            view!{cx, <LoginRedirect/>}}>
        { move || alerts.read(cx).ok_or(ServerFnError::ServerError("not logged in?".to_owned())).and_then(move |res| {
            res.map( move |alerts|
                view! {cx,
                    <ul class="">
                    <For each=move || alerts.clone()
                         key=|a| a.0.clone()
                         view=|cx, a| {
                             view! {cx,
                                 <li>
                                 <A class="hover:text-blue-400 hover:underline" href=move || format!("{}/update", a.0)>{a.1.name}</A>
                                 </li>
                             }
                         }
                    />
                    </ul>
                })
        } ) }
        </ErrorBoundary>
        </Suspense>
    }
}
