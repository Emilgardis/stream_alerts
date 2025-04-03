pub mod list;
pub mod login;
pub mod new;
pub mod update;

use list::*;
use login::*;
use new::*;
use update::*;

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::*;

#[cfg(feature = "ssr")]
#[derive(Clone, axum::extract::FromRef)]
pub struct AppState {
    pub leptos_options: LeptosOptions,
    pub alert_manager: new::AlertManager,
}


use crate::auth::User;

#[component()]
#[track_caller]
pub fn App() -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/site.css"/>
        //<Title text="Welcome to Leptos"/>
        <Router >
            <main class="flex items-center justify-center">
                <Routes
                fallback=|| view! { <p>"Loading..."</p> }
                >
                    <Route
                        path=path!("/alert")
                        view=|| view!{<ListAlerts/>}
                    />
                    <Route ssr=SsrMode::OutOfOrder
                        path=path!("/alert/:id/update")
                        view=|| view! { <UpdateAlert/> }
                    />
                    <Route ssr=SsrMode::OutOfOrder
                        path=path!("/alert/new")
                        view=|| view! { <NewAlert/> }
                    />
                    <Route ssr=SsrMode::OutOfOrder
                        path=path!("/login")
                        view=move || view! { <Login/> }
                    />
                </Routes>
            </main>
        </Router>
    }
}