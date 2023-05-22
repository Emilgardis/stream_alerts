use std::collections::BTreeMap;

use leptos::*;
use leptos_meta::*;
use leptos_router::*;

pub use super::login::*;
pub use crate::alerts::*;

#[component]
#[track_caller]
pub fn UpdateAlert(cx: Scope) -> impl IntoView {
    let params = use_params_map(cx);

    let alert = create_blocking_resource(
        cx,
        move || params.with(|p| p.get("id").cloned().unwrap_or_default().into()),
        move |id| async move { crate::alerts::read_alert(cx, id).await },
    );

    let update_alert_text = create_server_action::<UpdateAlertText>(cx);

    view! { cx,
        <div class="">
            <Suspense fallback=move || {
                view! { cx, <Title text="Update Alert"/><h1>"Update Alert"</h1> }
            }>
            //<Title text=move || alert.read(cx).map(|a| format!("Update Alert {}", a.name)).unwrap()/>
                <ErrorBoundary fallback=move |cx, _| view!{cx, <LoginRedirect/>}>
                {move || {
                    alert
                        .read(cx)
                        .map(|alert| alert.map(|alert| {
                            let alert = create_rw_signal(cx, alert);
                            provide_context(cx, alert);
                            view! { cx,
                                <h1>{move || format!("Update Alert {}", alert.get().name)}</h1>
                                <A href=move || format!("/alert/{}", alert.get().alert_id)>"View"</A>
                                <ActionForm action=update_alert_text class="bg-white rounded px-8 pt-6 pb-8 mb-4">
                                    <label for="alert_text">"Update text"</label>
                                    <textarea id="alert_text" name="text" class="">
                                        {alert.with(|a| a.last_text.to_string())}
                                    </textarea>
                                    <input
                                        type="submit"
                                        class="bg-blue-500 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded"
                                        value="Submit"
                                    />
                                    <input
                                        type="hidden"
                                        name="alert_id"
                                        value=move || alert.with(|a| a.alert_id.to_string())
                                    />
                                </ActionForm>
                                <AlertFields/>
                            }
                        }))
                }}
                </ErrorBoundary>
            </Suspense>
        </div>
    }
}

#[component]
#[track_caller]
pub fn AlertFields(cx: Scope) -> impl IntoView {
    let add_field = create_server_action::<AddAlertField>(cx);
    let delete_field = create_server_action::<DeleteAlertField>(cx);
    let update_field = create_server_action::<UpdateAlertField>(cx);
    let alert: RwSignal<Alert> = use_context(cx).unwrap();
    let fields = create_rw_signal(
        cx,
        alert
            .get()
            .fields
            .into_iter()
            .map(|(id, field)| (id, create_rw_signal(cx, field)))
            .collect::<BTreeMap<AlertFieldId, _>>(),
    );
    let delete_field_action = create_action(cx, move |key: &AlertFieldId| {
        let key = key.clone();
        async move {
            let cx = cx;
            delete_alert_field(cx, alert.with_untracked(|a| a.alert_id.clone()), key).await
        }
    });

    let _res = create_local_resource(
        cx,
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
            let new_fields = crate::alerts::read_alert(cx, id).await.expect("ehm").fields;
            let cx = cx;
            fields.update(|map| {
                map.retain(|k, _| new_fields.keys().any(|nk| nk == k));
                for (nk, nv) in new_fields.into_iter() {
                    map.entry(nk)
                        .and_modify(|v| {
                            if nv != v.get_untracked() {
                                v.set_untracked(nv.clone());
                            }
                        })
                        .or_insert_with(|| create_rw_signal(cx, nv));
                }
            })
        },
    );

    // list of AlertField's, with keys, using leptos For
    view! { cx,
        <div>
            <ActionForm action=add_field>
                <button type="submit">"Add field"</button>
                <select name="kind">
                    <option value="text">"text"</option>
                    <option value="counter">"counter"</option>
                </select>
                <AlertIdInput/>
                <input type="text" name="name" placeholder="name"/>
                <input type="text" name="value" placeholder="value"/>
            </ActionForm>
            <ul>
                <For
                    each=move || {let mut fields = fields.get().into_iter().collect::<Vec<_>>();
                        fields.sort_by_key(|(_id, signal)| signal.get().0);
                        fields
                    }
                    key=|value| value.0.clone()
                    view=move |cx, (name, field)| {
                        view! { cx,
                            <li>
                                <AlertField
                                    id=name.clone()
                                    on_delete=move |_| { delete_field_action.dispatch(name.clone()) }
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
    cx: Scope,
    on_delete: Delete,
    id: AlertFieldId,
    update_action: Action<UpdateAlertField, Result<Alert, ServerFnError>>,
    field: RwSignal<(AlertFieldName, AlertField)>,
) -> impl IntoView
where
    Delete: Fn(leptos::ev::MouseEvent) + 'static,
{
    view! { cx,
        <div>
            <button class="rounded border-2 border-red-500" on:click=on_delete>"x"</button>
            <ActionForm action=update_action>
            <AlertIdInput/>
            <input type="hidden" name="field_id" value=id/>
            <input type="text" name="field_name" value=move || field.get().0/>
            {move || match field.get().1 {
                AlertField::Text(value) => {
                    view! { cx,
                        <input type="text" name="value" value=value/>
                    }.into_any()
                }
                AlertField::Counter(value) => {
                    view! { cx,
                        <input type="number" name="value" value=value/>
                    }.into_any()
                }
            }}
            <input type="submit" value="Update"/>
            </ActionForm>
        </div>
    }
}

#[component]
pub fn AlertIdInput(cx: Scope) -> impl IntoView {
    let alert = use_context::<RwSignal<Alert>>(cx).unwrap();
    view! { cx,
        <input
            type="hidden"
            name="alert_id"
            value=move ||alert.with(|a| a.alert_id.clone())/>
    }
}

#[server(UpdateAlertName, "/backend")]
#[tracing::instrument(err)]
pub async fn update_alert_name(
    cx: Scope,
    alert_id: AlertId,
    name: String,
) -> Result<Alert, leptos::ServerFnError> {
    let Some(manager): Option<AlertManager> = leptos::use_context(cx) else {
        return Err(leptos::ServerFnError::ServerError("Missing manager".to_owned()));
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
pub async fn update_alert_text(
    cx: Scope,
    alert_id: AlertId,
    text: String,
) -> Result<Alert, leptos::ServerFnError> {
    let Some(manager): Option<AlertManager> = leptos::use_context(cx) else {
        return Err(leptos::ServerFnError::ServerError("Missing manager".to_owned()));
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

#[server(UpdateAlertField, "/backend")]
#[tracing::instrument(err)]
pub async fn update_alert_field(
    cx: Scope,
    alert_id: AlertId,
    field_name: Option<AlertFieldName>,
    field_id: AlertFieldId,
    value: String,
) -> Result<Alert, leptos::ServerFnError> {
    let Some(manager): Option<AlertManager> = leptos::use_context(cx) else {
        return Err(leptos::ServerFnError::ServerError("Missing manager".to_owned()));
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
        .map_err(|e: eyre::Report| leptos::ServerFnError::ServerError(e.to_string()))??;

    let map_r = manager.read_alerts().await;
    let alert = map_r.get(&alert_id).expect("no alert found");
    Ok(alert.clone())
}

#[server(DeleteAlertField, "/backend")]
#[tracing::instrument(err)]
pub async fn delete_alert_field(
    cx: Scope,
    alert_id: AlertId,
    field: AlertFieldId,
) -> Result<Alert, leptos::ServerFnError> {
    let Some(manager): Option<AlertManager> = leptos::use_context(cx) else {
        return Err(leptos::ServerFnError::ServerError("Missing manager".to_owned()));
    };

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
    cx: Scope,
    alert_id: AlertId,
    name: AlertFieldName,
    kind: String,
    value: String,
) -> Result<Alert, leptos::ServerFnError> {
    let Some(manager): Option<AlertManager> = leptos::use_context(cx) else {
        return Err(leptos::ServerFnError::ServerError("Missing manager".to_owned()));
    };

    manager
        .try_edit_alert(&alert_id, move |alert| {
            alert.add_alert_field(name, &kind, value)
        })
        .await
        .map_err(|e| leptos::ServerFnError::ServerError(e.to_string()))??;

    let map_r = manager.read_alerts().await;
    let alert = map_r.get(&alert_id).expect("no alert found");
    Ok(alert.clone())
}

#[cfg(feature = "ssr")]
pub(crate) fn register_server_fns() {
    _ = UpdateAlertText::register();
    _ = UpdateAlertName::register();
    _ = UpdateAlertField::register();
    _ = DeleteAlertField::register();
    _ = AddAlertField::register();
}
