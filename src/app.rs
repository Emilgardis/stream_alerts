pub mod new;
pub mod update;

use new::*;
use update::*;

use leptos::*;
use leptos_meta::*;
use leptos_router::*;

#[component]
#[track_caller]
pub fn App(cx: Scope) -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context(cx);

    view! { cx,
        <Stylesheet id="leptos" href="/pkg/site.css"/>
        //<Title text="Welcome to Leptos"/>
        <Router>
            <main>
                <Routes>
                    <Route
                        path="/alert/:id/update"
                        view=|cx| {
                            view! { cx, <UpdateAlert/> }
                        }
                    />
                    <Route
                        path="/alert/new"
                        view=|cx| {
                            view! { cx, <NewAlert/> }
                        }
                    />
                </Routes>
            </main>
        </Router>
    }
}

#[cfg(feature = "ssr")]
pub fn register_server_fns() {
    tracing::info!("registering server fns");
    update::register_server_fns();
    new::register_server_fns();
    _ = super::alerts::ReadAlert::register();
}
