use leptos::*;
use leptos_router::*;

pub use crate::alerts::*;

#[component]
#[track_caller]
pub fn NewAlert(cx: Scope) -> impl IntoView {
    let new_alert = create_server_action::<NewAlert>(cx);

    view! {cx,
        //<Title text="New Alert"/>
        <h1>"New Alert"</h1>
        <ActionForm action=new_alert>
            <input type="text" name="name" placeholder="Name"/>
            <input type="submit" value="Submit"/>
        </ActionForm>
        <Show when=move || {new_alert.value().get().is_some()} fallback=|_cx| view!{_cx, ""}>
            {move || {
                let value = new_alert.value().get().expect("wtf");
                view!{ cx,
                    <ErrorBoundary
                    // the fallback receives a signal containing current errors
                    fallback=|cx, errors| view! { cx,
                        <div class="error">
                            <p>"Couldn't create alert! Error: "</p>
                            // we can render a list of errors as strings, if we'd like
                            <ul>
                                {move || errors.get()
                                    .into_iter()
                                    .map(|(_, e)| view! { cx, <li>{e.to_string()}</li>})
                                    .collect_view(cx)
                                }
                            </ul>
                        </div>
                    }
                >
                {value.map(|id| view! {cx, <A href=move || format!("/alert/{id}/update")>"Alert Created"</A>})}
                </ErrorBoundary>
                }
            }}
        </Show>
        <A href="/alert/X7IRXaNgIQ6eb5sWHqIqL/update">"Back to Alerts"</A>
    }
}

#[server(NewAlert, "/backend")]
#[tracing::instrument(err)]
pub async fn new_alert(cx: Scope, name: String) -> Result<AlertId, leptos::ServerFnError> {
    let Some(manager): Option<AlertManager> = leptos::use_context(cx) else {
        return Err(leptos::ServerFnError::ServerError("Missing manager".to_owned()));
    };

    let id = AlertId::new_id();
    manager
        .new_alert(Alert::new(id.clone(), AlertText::from(""), name.into()))
        .await?;

    Ok(id)
}

#[cfg(feature = "ssr")]
pub(crate) fn register_server_fns() { _ = NewAlert::register(); }
