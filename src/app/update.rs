use std::collections::BTreeMap;

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{components::A, *};
use reactive_stores::{Field, Store, StoreField};

pub use super::login::*;
pub use crate::alerts::*;

#[track_caller]
#[component()]
pub fn UpdateAlert() -> impl IntoView {
    let params = hooks::use_params_map();
    let alert_id = params.read_untracked().get("id").expect("no id").to_owned();
    let store = Store::<Alert>::new(Alert {
        alert_id: alert_id.clone().into(),
        name: "loading...".into(),
        last_text: "".into(),
        last_style: "".into(),
        fields: Vec::new(),
    });

    let _r = Resource::new(
        move || params.read().get("id").expect("no id"),
        move |id| async move {
            let alert = read_alert(store.alert_id().get_untracked())
                .await
                .expect("no alert found");
            *store.write() = alert;
        },
    );

    view! {
        <ErrorBoundary fallback=move |e| view! { <p>{move || format!("error: {e:?}")}</p> }>
        <div class="mx-auto flex max-w-4xl flex-col gap-6 p-6">
             <AlertName store/>
             <AlertTextInputs store/>
             <AlertFieldInputs store/>
        </div>
        </ErrorBoundary>
    }
}

async fn provide_store(store: Store<Alert>) {}

#[component]
pub fn AlertName(store: Store<Alert>) -> impl IntoView {
    let update_alert_name = ServerAction::<UpdateAlertName>::new();

    view! {
        <div class="flex items-center justify-between">
            <ActionForm action=update_alert_name>
            <h2 class="text-2xl font-bold flex items-center gap-2">Editing Alert:
                <input type="text" name="alert_name" value=move || store.name().read().to_string() class="text-blue-600 font-bold border-b border-blue-300 bg-transparent focus:outline-none focus:border-blue-500" />
            </h2>
            <button class="rounded bg-red-500 px-4 py-2 text-sm text-white hover:bg-red-600">Delete Alert</button>
            </ActionForm>
        </div>
    }
}

#[component]
pub fn AlertTextInputs(store: Store<Alert>) -> impl IntoView {
    let update_alert_text = ServerAction::<UpdateAlertText>::new();
    let update_alert_style = ServerAction::<UpdateAlertStyle>::new();
    view! {
        <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
        <ActionForm attr:class="relative" action=update_alert_text>
          <label class="block text-sm font-medium text-gray-700">Alert Markdown</label>
          <textarea id="text" class="mb-10 h-40 w-full resize-y rounded border p-2 font-mono text-sm" placeholder="Enter markdown..." name="text">{move ||store.read().render().to_string()}</textarea>
          <div class="absolute right-2 bottom-2 z-10">
            <button class="rounded bg-blue-600 px-4 py-1 text-sm text-white shadow hover:bg-blue-700" type="submit">Save</button>
          </div>
          <div class="absolute -top-6 left-0 hidden rounded bg-yellow-100 px-3 py-1 text-xs text-yellow-800 shadow-sm">Field updated elsewhere - <button class="underline">click to refresh</button></div>
          <AlertIdInput store/>
        </ActionForm>

        <ActionForm attr:class="relative" action=update_alert_style>
          <label class="block text-sm font-medium text-gray-700">Custom CSS</label>
          <textarea id="style" class="mb-10 h-40 w-full resize-y rounded border p-2 font-mono text-sm" placeholder="Enter CSS..." name="style">{move || store.read().render_style().to_string()}</textarea>
          <div class="absolute right-2 bottom-2 z-10">
            <button class="rounded bg-blue-600 px-4 py-1 text-sm text-white shadow hover:bg-blue-700" type="submit">Save</button>
          </div>
          <div class="absolute -top-6 left-0 hidden rounded bg-yellow-100 px-3 py-1 text-xs text-yellow-800 shadow-sm">Field updated elsewhere - <button class="underline">click to refresh</button></div>
          <AlertIdInput store/>
        </ActionForm>
      </div>
    }
}

#[component]
pub fn AlertFieldInputs(store: Store<Alert>) -> impl IntoView {
    view! {
        <h3 class="mb-2 text-lg font-semibold">Fields</h3>
        <div class="flex flex-col gap-3">
            <For each=move || store.fields()
                key=|field| field.read().0.clone()
                let(field)
            >
                <AlertField store field/>
            </For>
        </div>
    }
}

#[component]
pub fn AlertField(
    store: Store<Alert>,
    field: reactive_stores::AtKeyed<
        Store<Alert>,
        Alert,
        AlertFieldId,
        Vec<(AlertFieldId, (AlertFieldName, AlertField))>,
    >,
) -> impl IntoView {
    let update_alert_field = ServerAction::<UpdateAlertField>::new();
    let (f1, f2, f3, f4, f5) = (
        field.clone(),
        field.clone(),
        field.clone(),
        field.clone(),
        field.clone(),
    );
    let delete = Action::new(|(field_id, store): &(AlertFieldId, Store<Alert>)| {
        let field_id = field_id.to_owned();
        let store = store.clone();
        async move {
            delete_alert_field(store.alert_id().get(), field_id).await;
        }
    });
    view! {
        <ActionForm attr:class="relative rounded border bg-gray-50 p-4 shadow-sm" action=update_alert_field>
            <div class="flex items-center justify-between">
                <h4 class="font-medium">{move || f1.read().1.0.to_string()}</h4>
                <button class="text-sm text-red-500 hover:underline" type="button" on:click=move |_| {
                        let field_id = &f2.read().0;
                        delete.dispatch((field_id.clone(), store));
                        store.fields().write().retain(|f| &f.0 != field_id);
                    }>Remove</button>
            </div>
            <div class="mt-2">
                <input type={move || match f3.read().1.1 {
                    AlertField::Text(_) => "text",
                    AlertField::Counter(_) => "number",
                }}
                class="w-full rounded border p-2"
                name={move || f4.read().1.0.to_string()}
                value={move || f5.read().1.1.to_string()} />
            </div>
                <div class="mt-2 flex items-center justify-between">
                <div class="hidden rounded bg-yellow-100 px-3 py-1 text-xs text-yellow-800 shadow-sm">Field updated elsewhere - <button class="underline">click to refresh</button></div>
                <button class="rounded bg-blue-600 px-4 py-1 text-sm text-white shadow hover:bg-blue-700" type="submit">Save Field</button>
            </div>
        </ActionForm>
    }
}

#[component]
pub fn AlertIdInput(store: Store<Alert>) -> impl IntoView {
    view! {
        <input
            type="hidden"
            name="alert_id"
            value={ move || store.alert_id().get().to_string()}/>
    }
}

#[server(UpdateAlertName, "/backend")]
#[tracing::instrument(err)]
pub async fn update_alert_name(alert_id: AlertId, name: String) -> Result<Alert, ServerFnError> {
    let Some(manager): Option<AlertManager> = use_context() else {
        return Err(ServerFnError::ServerError("Missing manager".to_owned()));
    };

    manager
        .edit_alert(&alert_id, move |alert| {
            alert.name = name.into();
        })
        .await?;

    let map_r = manager.read_alerts().await;
    let alert = map_r.get(&alert_id).expect("no alert found");
    Ok(alert.clone())
}

#[server(UpdateAlertText, "/backend")]
#[tracing::instrument(err)]
pub async fn update_alert_text(alert_id: AlertId, text: String) -> Result<Alert, ServerFnError> {
    let Some(manager): Option<AlertManager> = use_context() else {
        return Err(ServerFnError::ServerError("Missing manager".to_owned()));
    };

    manager
        .edit_alert(&alert_id, move |alert| {
            alert.last_text = text.into();
        })
        .await?;

    let map_r = manager.read_alerts().await;
    let alert = map_r.get(&alert_id).expect("no alert found");
    Ok(alert.clone())
}

#[server(UpdateAlertStyle, "/backend")]
#[tracing::instrument(err)]
pub async fn update_alert_style(alert_id: AlertId, style: String) -> Result<Alert, ServerFnError> {
    let Some(manager): Option<AlertManager> = use_context() else {
        return Err(ServerFnError::ServerError("Missing manager".to_owned()));
    };

    manager
        .edit_alert(&alert_id, move |alert| {
            alert.last_style = style;
        })
        .await?;

    let map_r = manager.read_alerts().await;
    let alert = map_r.get(&alert_id).expect("no alert found");
    Ok(alert.clone())
}

#[server(UpdateAlertField, "/backend")]
#[tracing::instrument(err)]
pub async fn update_alert_field(
    alert_id: AlertId,
    field_name: Option<AlertFieldName>,
    field_id: AlertFieldId,
    value: String,
) -> Result<Alert, ServerFnError> {
    let Some(manager): Option<AlertManager> = use_context() else {
        return Err(ServerFnError::ServerError("Missing manager".to_owned()));
    };

    manager
        .try_edit_alert(&alert_id, move |alert| {
            if let Some(mut entry) = alert.fields.iter_mut().find(|f| f.0 == field_id) {
                entry.1 .1.set(value)?;
                if let Some(new_field_name) = field_name {
                    entry.1 .0 = new_field_name;
                }
            }
            Ok(())
        })
        .await
        .map_err(|e: eyre::Report| {
            ServerFnError::<leptos::server_fn::error::NoCustomError>::ServerError(e.to_string())
        })??;

    let map_r = manager.read_alerts().await;
    let alert = map_r.get(&alert_id).expect("no alert found");
    Ok(alert.clone())
}

#[server(DeleteAlertField, "/backend")]
#[tracing::instrument(err)]
pub async fn delete_alert_field(
    alert_id: AlertId,
    field: AlertFieldId,
) -> Result<Alert, ServerFnError> {
    let Some(manager): Option<AlertManager> = use_context() else {
        return Err(ServerFnError::ServerError("Missing manager".to_owned()));
    };
    tracing::info!(?alert_id, ?field, "deleted field");

    manager
        .edit_alert(&alert_id, move |alert| {
            alert.fields.retain(|k| &k.0 != &field);
        })
        .await?;

    let map_r = manager.read_alerts().await;
    let alert = map_r.get(&alert_id).expect("no alert found");
    Ok(alert.clone())
}

#[server(AddAlertField, "/backend")]
#[tracing::instrument(err)]
pub async fn add_alert_field(
    alert_id: AlertId,
    name: AlertFieldName,
    kind: String,
    value: String,
) -> Result<Alert, ServerFnError> {
    let Some(manager): Option<AlertManager> = use_context() else {
        return Err(ServerFnError::ServerError("Missing manager".to_owned()));
    };

    manager
        .try_edit_alert(&alert_id, move |alert| {
            alert.add_alert_field(name, &kind, value)
        })
        .await
        .map_err(|e| {
            ServerFnError::<leptos::server_fn::error::NoCustomError>::ServerError(e.to_string())
        })??;

    let map_r = manager.read_alerts().await;
    let alert = map_r.get(&alert_id).expect("no alert found");
    Ok(alert.clone())
}
