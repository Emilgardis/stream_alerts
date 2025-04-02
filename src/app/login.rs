use leptos::prelude::*;
use leptos_router::{
    components::{Redirect, A},
    *,
};

pub use crate::alerts::*;

#[component]
#[track_caller]
pub fn Login() -> impl IntoView {
    let login = ServerAction::<LoginUser>::new();
    let query = hooks::use_params_map();
    let redirect = move || {
        query.read()
            .get("redirect")
            .map(|s| s.to_owned())
            .unwrap_or_else(|| "/alert".to_owned())
    };

    view! {
        //<Title text="New Alert"/>
        <h1>"Login"</h1>
        <ActionForm action=login>
            <input type="text" name="username" placeholder="Name"/>
            <input type="password" name="password" placeholder="Password"/>
            <input type="submit" value="Submit"/>
        </ActionForm>
        <ErrorBoundary fallback=|errors| {view!{{format!("Nice try\n{:?}", errors.get().iter().next().map(|e| e.1.to_string()).unwrap_or_default())}}}>
        {move || login.value().get().map(|res| res.map(|_| view!(  <p>"Logged in!"</p><Redirect path=redirect()/>))
        )}
        </ErrorBoundary>
    }
}

#[component]
pub fn LoginRedirect() -> impl IntoView {
    view! {<p>"Access Denied!"</p><A href=move || format!("/login?redirect={}", location_pathname().unwrap_or_default())>"Login?"</A>}
}
#[server(LoginUser, "/backend/public")]
#[tracing::instrument(err)]
pub async fn login(username: String, password: String) -> Result<bool, ServerFnError> {
    let mut auth = use_context::<crate::auth::AuthSession>().expect("wtf");

    let res_options_outer = use_context::<leptos_axum::ResponseOptions>();
    tracing::info!("got login request");
    if let Some(res_options) = res_options_outer {
        let Some(user) = auth
            .authenticate((username, password.as_bytes().to_vec()))
            .await?
        else {
            tracing::info!("user not found");
            return Err(
                ServerFnError::<leptos::server_fn::error::NoCustomError>::ServerError(
                    "user not found or password wrong".to_owned(),
                ),
            );
        };

        auth.login(&user).await?;
        provide_context::<crate::auth::User>(user.clone());
        tracing::info!(?user, "logged in");

        Ok(true)
    } else {
        Err(ServerFnError::ServerError("Oops!".to_owned()))
    }
}
