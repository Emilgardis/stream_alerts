use leptos::*;
use leptos_router::*;

pub use crate::alerts::*;

#[component]
#[track_caller]
pub fn Login(cx: Scope, user: RwSignal<Option<crate::auth::User>>) -> impl IntoView {
    let login = create_server_action::<LoginUser>(cx);
    view! {cx,
        //<Title text="New Alert"/>
        <h1>"Login"</h1>
        <ActionForm action=login>
            <input type="text" name="name" placeholder="Name"/>
            <input type="password" name="password" placeholder="Password"/>
            <input type="submit" value="Submit"/>
        </ActionForm>
    }
}

#[server(LoginUser, "/backend/public")]
#[tracing::instrument(err)]
pub async fn login(
    cx: Scope,
    name: String,
    password: String,
) -> Result<bool, leptos::ServerFnError> {
    use cookie;
    let mut auth = use_context::<crate::auth::AuthContext>(cx).expect("wtf");

    let res_options_outer = use_context::<leptos_axum::ResponseOptions>(cx);
    if let Some(res_options) = res_options_outer {
        if name == "emil" {
            auth.login(&crate::auth::User {
                name: "emil".into(),
                id: 0,
                password_hash: vec![],
            }).await.unwrap();
        }

        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(feature = "ssr")]
pub(crate) fn register_server_fns() {
    _ = LoginUser::register();
}
