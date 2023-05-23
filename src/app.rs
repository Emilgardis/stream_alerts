pub mod list;
pub mod login;
pub mod new;
pub mod update;

use list::*;
use login::*;
use new::*;
use update::*;

use leptos::*;
use leptos_meta::*;
use leptos_router::*;

use crate::auth::User;

#[component]
#[track_caller]
pub fn App(cx: Scope) -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context(cx);

    let user = create_rw_signal::<Option<User>>(cx, None);

    view! { cx,
        <Stylesheet id="leptos" href="/pkg/site.css"/>
        //<Title text="Welcome to Leptos"/>
        <Router >
            <main>
                <Routes>
                    <Route
                        path="/alert"
                        view=|cx| view!{cx, <ListAlerts/>}
                    />
                    <Route ssr=SsrMode::OutOfOrder
                        path="/alert/:id/update"
                        view=|cx| view! { cx, <UpdateAlert/> }
                    />
                    <Route ssr=SsrMode::OutOfOrder
                        path="/alert/new"
                        view=|cx| view! { cx, <NewAlert/> }
                    />
                    <Route ssr=SsrMode::OutOfOrder
                        path="/login"
                        view=move |cx| view! { cx, <Login user=user/> }
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
    login::register_server_fns();
    _ = super::alerts::ReadAlert::register();
    _ = super::alerts::ReadAllAlerts::register();
}
