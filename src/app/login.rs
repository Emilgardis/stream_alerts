use leptos::*;
use leptos_router::*;

pub use crate::alerts::*;

#[component]
#[track_caller]
pub fn Login(cx: Scope, user: RwSignal<Option<crate::auth::User>>) -> impl IntoView {
    let login = create_server_action::<LoginUser>(cx);
    let query = use_location(cx).query;
    let redirect = move || {
        query
            .with(|p| p.get("redirect").map(|s| s.to_owned()))
            .unwrap_or_else(|| "/alert".to_owned())
    };

    view! {cx,
        //<Title text="New Alert"/>
        <h1>"Login"</h1>
        <ActionForm action=login>
            <input type="text" name="username" placeholder="Name"/>
            <input type="password" name="password" placeholder="Password"/>
            <input type="submit" value="Submit"/>
        </ActionForm>
        <ErrorBoundary fallback=|_c, errors| {view!{_c, {format!("Nice try\n{:?}", errors.get().iter().next().map(|e| e.1.to_string()).unwrap_or_default())}}}>
        {move || login.value().get().map(|res| res.map(|_| view!(cx,  <p>"Logged in!"</p><Redirect path=redirect()/>)))}
        </ErrorBoundary>
    }
}

#[component]
pub fn LoginRedirect(cx: Scope) -> impl IntoView {
    let location = use_location(cx);

    view! {cx, <p>"Access Denied!"</p><A class="hover:underline" href=move || format!("/login?redirect={}", location.pathname.get())>"Login?"</A>}
}
#[server(LoginUser, "/backend/public")]
#[tracing::instrument(err)]
pub async fn login(
    cx: Scope,
    username: String,
    password: String,
) -> Result<bool, leptos::ServerFnError> {
    let users = use_context::<crate::auth::Users>(cx).expect("wtf");
    let mut auth = use_context::<crate::auth::AuthContext>(cx).expect("wtf");

    let res_options_outer = use_context::<leptos_axum::ResponseOptions>(cx);
    let req_parts = use_context::<leptos_axum::RequestParts>(cx);
    if let Some(req) = req_parts {
        tracing::info!(?req, "got request");
    }
    if let Some(res_options) = res_options_outer {
        let Some(user) = users.get(&username, password.as_bytes()).await else {
            tracing::info!("user not found");
            return Err(ServerFnError::ServerError("user not found or password wrong".to_owned()))
        };

        auth.login(&user).await.unwrap();
        tracing::info!(?auth, "logged in");

        Ok(true)
    } else {
        Err(ServerFnError::ServerError("Oops!".to_owned()))
    }
}

#[cfg(feature = "ssr")]
pub(crate) fn register_server_fns() { _ = LoginUser::register(); }
