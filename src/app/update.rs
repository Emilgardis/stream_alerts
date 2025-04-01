use std::collections::BTreeMap;

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{components::A, *};

pub use super::login::*;
pub use crate::alerts::*;

#[track_caller]
#[component()]
pub fn UpdateAlert() -> impl IntoView {
    let params = hooks::use_params_map();

    let alert = Resource::new_blocking(
        move || params.read().get("id").unwrap_or_default().into(),
        move |id| async move { crate::alerts::read_alert(id).await },
    );

    let update_alert_text = ServerAction::<UpdateAlertText>::new();
    let update_alert_style = ServerAction::<UpdateAlertStyle>::new();
    let update_alert_name = ServerAction::<UpdateAlertName>::new();

    view! {
        <div class="">
            <Suspense fallback=move || {
                view! {  <Title text="Update Alert"/><h1>"Update Alert"</h1> }
            }>
            //<Title text=move || alert.read().map(|a| format!("Update Alert {}", a.name)).unwrap()/>
                <ErrorBoundary fallback=move | _| view!{ <LoginRedirect/>}>
                {move || {
                    view! {{
                        match alert.read().as_ref() {
                            Some(Ok(alert)) => {
                                let alert = RwSignal::new(alert.clone());
                                provide_context(alert);
                                view! {
                                    <div class="w-full max-w-xl bg-white shadow rounded-lg p-6">
                                    <h1>"Update Alert" <div class="flex-col"><ActionForm action=update_alert_name>
                                        <AlertIdInput/>
                                        <input type="text" name="name" class="border-none" value=move || alert.with(|a| a.name.to_string())/>
                                        <input class="hover:underline cursor-pointer border-none" type="submit" value="Change name"/>
                                    </ActionForm></div> </h1>
                                    <div class="text-blue-500 hover:text-blue-700 underline text-sm" ><A href=move || format!("/alert/{}", alert.get().alert_id)>"View"</A></div>
                                    <div class="flex flex-col mb-4"><ActionForm action=update_alert_text >
                                        <label class="font-semibold text-gray-900" for="alert_text">"Update text"</label>
                                        <textarea id="alert_text" name="text" class="border-2 border-gray-200 rounded p-2 h-48">
                                            {move || alert.with(|a| a.last_text.to_string())}
                                        </textarea>
                                        <input
                                            type="submit"
                                            class="cursor-pointer border-none bg-blue-500 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded"
                                            value="Submit"
                                        />
                                        <AlertIdInput/>
                                    </ActionForm></div>
                                    <div class="flex flex-col mb-4"><ActionForm action=update_alert_style >
                                        <label class="font-semibold text-gray-900" for="alert_text">"Update style"</label>
                                        <textarea id="alert_style" name="style" class="border-2 border-gray-200 rounded p-2 h-48">
                                            {move || alert.with(|a| a.last_style.to_string())}
                                        </textarea>
                                        <input
                                            type="submit"
                                            class="cursor-pointer border-none bg-blue-500 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded"
                                            value="Submit"
                                        />
                                        <AlertIdInput/>
                                    </ActionForm></div>
                                    <AlertFields/>
                                    </div>
                                }.into_any()
                            }
                            Some(Err(e)) => view! {
                                <p class="text-red-500">{format!("Error: {e}")}</p>
                            }.into_any(),
                            None => view! {
                                <p>"Loading..."</p>
                            }.into_any(),
                        }
                    }}
                }}
                </ErrorBoundary>
            </Suspense>
        </div>
    }
}

#[component()]
#[track_caller]
pub fn AlertFields() -> impl IntoView {
    let add_field = ServerAction::<AddAlertField>::new();
    let delete_field = ServerAction::<DeleteAlertField>::new();
    let update_field = ServerAction::<UpdateAlertField>::new();
    let alert: RwSignal<Alert> = use_context().unwrap();
    let fields = RwSignal::new(
        alert
            .get_untracked()
            .fields
            .into_iter()
            .map(|(id, field)| (id, RwSignal::new(field)))
            .collect::<BTreeMap<AlertFieldId, _>>(),
    );
    let delete_field_action = Action::new(move |key: &AlertFieldId| {
        let key = key.clone();
        async move { delete_alert_field(alert.with_untracked(|a| a.alert_id.clone()), key).await }
    });

    let _res = Resource::new(
        move || {
            (
                alert.with(|a| a.alert_id.clone()),
                add_field.version().get(),
                delete_field.version().get(),
                delete_field_action.version().get(),
                update_field.version().get(),
            )
        },
        move |(id, ..)| async move {
            let new_fields = crate::alerts::read_alert(id).await.expect("ehm").fields;
            fields.update(|map| {
                map.retain(|k, _| new_fields.keys().any(|nk| nk == k));
                for (nk, nv) in new_fields.into_iter() {
                    map.entry(nk)
                        .and_modify(|v| {
                            if nv != v.get_untracked() {
                                v.set(nv.clone());
                            }
                        })
                        .or_insert_with(|| RwSignal::new(nv));
                }
            })
        },
    );

    // list of AlertField's, with keys, using leptos For
    view! {
        <div class="flex items-start space-x-4 mb-4" >
            <ActionForm action=add_field>
                <AlertIdInput/>
                <div class="flex flex-col space-y-4 mr-4">
                <button class="cursor-pointer bg-blue-500 hover:bg-blue-700 text-white font-bold py-1 px-2 rounded text-sm" type="submit">"Add field"</button>
                <select class = "border border-gray-300 rounded px-4 py-2" name="kind">
                    <option value="text">"text"</option>
                    <option value="counter">"counter"</option>
                </select>
                </div>
                <div class="flex flex-col space-y-4 flex-grow">
                <input class = "border border-gray-300 rounded px-4 py-2" type="text" name="name" placeholder="name"/>
                <input class = "border border-gray-300 rounded px-4 py-2" type="text" name="value" placeholder="value"/>
                </div>
            </ActionForm>
            <ul>
                <For
                    each=move || {let mut fields = fields.get().into_iter().collect::<Vec<_>>();
                        fields.sort_by_key(|(_id, signal)| signal.get().0);
                        fields
                    }
                    key=|value| value.0.clone()
                    children=move | (name, field)| {
                        view! {
                            <li>
                                <AlertField
                                    id=name.clone()
                                    on_delete=move |_| { delete_field_action.dispatch(name.clone()); }
                                    update_action=update_field
                                    field=field
                                />
                            </li>
                        }
                    }
                />
            </ul>
        </div>
    }
}

#[component]
#[track_caller]
pub fn AlertField<Delete>(
    on_delete: Delete,
    id: AlertFieldId,
    update_action: ServerAction<UpdateAlertField>,
    field: RwSignal<(AlertFieldName, AlertField)>,
) -> impl IntoView
where
    Delete: Fn(leptos::ev::MouseEvent) + 'static,
{
    view! {
        <div>
        <div class="flex flex-row">
        //<button class="cursor-pointer py-2 rounded border-2 border-red-500 hover:border-red-900" on:click=on_delete>"𐄂"</button>

        <div class="contents"><ActionForm action=update_action>
            <AlertIdInput/>
            <input type="hidden" name="field_id" value=id/>
            <input class="border border-gray-300 rounded px-4 py-2" type="text" name="field_name" value={move || field.get().0.to_string()}/>
            {move || match field.get().1 {
                AlertField::Text(value) => {
                    view! {
                        <input class="border border-gray-300 rounded px-4 py-2" type="text" name="value" value=value/>
                    }.into_any()
                }
                AlertField::Counter(value) => {
                    view! {
                        <input class="border border-gray-300 rounded px-4 py-2" type="number" name="value" value=value/>
                    }.into_any()
                }
            }}
            <input class="rounded bg-blue-500 hover:bg-blue-700 text-white" type="submit" value="✓"/>
        </ActionForm></div>
        </div>
        </div>
    }
}

#[component]
pub fn AlertIdInput() -> impl IntoView {
    let alert = use_context::<RwSignal<Alert>>().unwrap();
    view! {
        <input
            type="hidden"
            name="alert_id"
            value={ move || alert.with(|a| a.alert_id.to_string())}/>
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
            alert.last_style = style.into();
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
            if let std::collections::btree_map::Entry::Occupied(mut entry) =
                alert.fields.entry(field_id)
            {
                entry.get_mut().1.set(value)?;
                if let Some(new_field_name) = field_name {
                    entry.get_mut().0 = new_field_name;
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
            alert.fields.remove(&field);
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
