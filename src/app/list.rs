use std::collections::BTreeMap;

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{components::A, *};

pub use super::login::*;
pub use crate::alerts::*;

#[component]
#[track_caller]
pub fn ListAlerts() -> impl IntoView {
    let params = hooks::use_params_map();

    let alerts = Resource::new_blocking(
        move || (),
        move |_| async move { crate::alerts::read_all_alerts().await },
    );

    view! {

        <p class="font-bold text-lg mb-4 text-center">"Alerts"</p>
        <Suspense fallback=move || view!{<p>"loading"</p>}>
        { move || {
            match alerts.read().clone() {
                Some(Ok(alerts)) => view! {
                    <ul class="bg-white shadow rounded-lg p-4">
                    <For each=move || alerts.clone()
                         key=|a| a.0.clone()
                         children=|a| {
                             view! {
                                 <li class="border-b border-gray-200 py-2">
                                 <div class="text-gray-700 hover:text-blue-400 hover:underline"><A href=move || format!("{}/update", a.0)>{a.1.name}</A></div>
                                 </li>
                             }
                         }
                    />
                    </ul>
                }.into_any(),
                _ => view! {
                }.into_any(),
            }
        }}
        </Suspense>
    }
}
