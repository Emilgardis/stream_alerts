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

use crate::auth::User;

#[component()]
#[track_caller]
pub fn App() -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context();

    let user = RwSignal::new(None);

    view! {
        <Stylesheet id="leptos" href="/pkg/site.css"/>
        //<Title text="Welcome to Leptos"/>
        <Router >
            <main class="flex items-center justify-center min-h-screen p-6 bg-gray-50">
            <div class="w-full max-w-xl">
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
                        view=move || view! { <Login user=user/> }
                    />
                </Routes>
            </div>
            </main>
        </Router>
    }
}