use leptos::prelude::*;
use leptos_router::components::A;

pub use crate::alerts::*;

#[component()]
#[track_caller]
pub fn NewAlert() -> impl IntoView {
    let new_alert = ServerAction::<NewAlert>::new();

    view! {
        //<Title text="New Alert"/>
        <h1>"New Alert"</h1>
        <ActionForm action=new_alert>
            <input type="text" name="name" placeholder="Name"/>
            <input type="submit" value="Submit"/>
        </ActionForm>
        <Show when=move || {new_alert.value().get().is_some()} fallback=|| view!{_ ""}>
            {move || {
                let value = new_alert.value().get().expect("wtf");
                view!{
                    <ErrorBoundary
                    // the fallback receives a signal containing current errors
                    fallback=| errors| view! {
                        <div class="error">
                            <p>"Couldn't create alert! Error: "</p>
                            // we can render a list of errors as strings, if we'd like
                            <ul>
                                {move || errors.get()
                                    .into_iter()
                                    .map(|(_, e)| view! {  <li>{e.to_string()}</li>})
                                    .collect_view()
                                }
                            </ul>
                        </div>
                    }
                >
                {value.map(|id| view! { <A href=move || format!("/alert/{id}/update")>"Alert Created"</A>})}
                </ErrorBoundary>
                }
            }}
        </Show>
        <A href="/alert/X7IRXaNgIQ6eb5sWHqIqL/update">"Back to Alerts"</A>
    }
}

#[server(NewAlert, "/backend")]
#[tracing::instrument(err)]
pub async fn new_alert(name: String) -> Result<AlertId, ServerFnError> {
    let Some(manager): Option<AlertManager> = use_context() else {
        return Err(ServerFnError::ServerError("Missing manager".to_owned()));
    };

    let id = AlertId::new_id();
    manager
        .new_alert(Alert::new(id.clone(), AlertText::from(""), name.into()))
        .await?;

    Ok(id)
}
