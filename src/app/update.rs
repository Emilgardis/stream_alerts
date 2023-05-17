use std::collections::BTreeMap;

use gloo_net::http::Method;
use gloo_utils::format::JsValueSerdeExt;
use leptos::*;
use leptos_meta::*;
use leptos_router::*;

pub use crate::alerts::*;

#[component]
#[track_caller]
pub fn UpdateAlert(cx: Scope) -> impl IntoView {
    let params = use_params_map(cx);

    let alert = create_resource(
        cx,
        move || params.with(|p| p.get("id").cloned().unwrap_or_default().into()),
        move |id| async move { crate::alerts::read_alert(cx, id).await.expect("ehm") },
    );

    let save_text = create_action(cx, move |text: &String| {
        // `task` is given as `&String` because its value is available in `input`
        let text = text.to_owned();
        async move {
            let Some(alert_id) = alert.read(cx).map(|a| a.alert_id) else {
                return Err("No alert found".into())
            };
            let fut = update_alert_text(cx, alert_id, text.to_owned());
            fut.await.map_err(|e| e.to_string()).map(|d| alert.set(d))
        }
    });

    let update_alert_text = create_server_action::<UpdateAlertText>(cx);

    tracing::info!(?alert);
    view! { cx,
        <div class="">
            <Suspense fallback=move || {
                view! { cx, <Title text="Update Alert"/><h1>"Update Alert"</h1> }
            }>
            <Title text=move || alert.read(cx).map(|a| format!("Update Alert {}", a.name)).unwrap()/>
                <h1>{move || alert.read(cx).map(|a| format!("Update Alert {}", a.name))}</h1>
                {move || {
                    alert
                        .read(cx)
                        .map(|alert| {
                            let alert = create_rw_signal(cx, alert);
                            provide_context(cx, alert);
                            view! { cx,
                                <ActionForm action=update_alert_text class="bg-white rounded px-8 pt-6 pb-8 mb-4">
                                    <label for="alert_text">"Update text"</label>
                                    <textarea id="alert_text" name="text">
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
                        })
                }}
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
            .map(|(name, field)| (name, create_rw_signal(cx, field)))
            .collect::<BTreeMap<AlertFieldName, _>>(),
    );
    let delete_value = create_action(cx, move |key: &AlertFieldName| {
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
                delete_value.version().get(),
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
                <input
                    type="hidden"
                    name="alert_id"
                    value=move || alert.with(|a| a.alert_id.to_string())
                />
                <input type="text" name="name" placeholder="name"/>
                <input type="text" name="value" placeholder="value"/>
            </ActionForm>
            <ul>
                <For
                    each=move || fields.get()
                    key=|value| value.1.with(|v| v.0.clone())
                    view=move |cx, (name, field)| {
                        view! { cx,
                            <li>
                                <AlertField
                                    name=name.clone()
                                    on_delete=move |_| { delete_value.dispatch(name.clone()) }
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
    name: AlertFieldName,
    update_action: Action<UpdateAlertField, Result<Alert, ServerFnError>>,
    field: RwSignal<(AlertFieldId, AlertField)>,
) -> impl IntoView
where
    Delete: Fn(leptos::ev::MouseEvent) + 'static,
{
    let alert_id = use_context::<RwSignal<Alert>>(cx)
        .unwrap()
        .with(|a| a.alert_id.clone());

    view! { cx,
        <div>
            <button class="rounded border-2 border-red-500" on:click=on_delete>"x"</button>
            <ActionForm action=update_action>
            <input
            type="hidden"
            name="alert_id"
            value={alert_id}/>
            <input type="text" name="field_name" value=name/>
            {move || match field.get().1 {
                AlertField::Text(value) => {
                    view! { cx,
                        <input type="text" name="value" value=value/>
                    }
                }
                AlertField::Counter(value) => {
                    view! { cx,
                        <input type="number" name="value" value=value/>
                    }
                }
            }}
            <input type="submit" value="Update"/>
            </ActionForm>
        </div>
    }
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

    let map_r = manager.alerts.read().await;
    let alert = map_r.get(&alert_id).expect("no alert found");
    Ok(alert.clone())
}

#[server(UpdateAlertField, "/backend")]
#[tracing::instrument(err)]
pub async fn update_alert_field(
    cx: Scope,
    alert_id: AlertId,
    field_name: AlertFieldName,
    value: String,
) -> Result<Alert, leptos::ServerFnError> {
    let Some(manager): Option<AlertManager> = leptos::use_context(cx) else {
        return Err(leptos::ServerFnError::ServerError("Missing manager".to_owned()));
    };

    manager
        .try_edit_alert(&alert_id, move |alert| {
            alert
                .fields
                .get_mut(&field_name)
                .ok_or(eyre::eyre!("no such field"))
                .and_then(|f| f.1.set(value))?;
            Ok(())
        })
        .await
        .map_err(|e: eyre::Report| leptos::ServerFnError::ServerError(e.to_string()))??;

    let map_r = manager.alerts.read().await;
    let alert = map_r.get(&alert_id).expect("no alert found");
    Ok(alert.clone())
}

#[server(DeleteAlertField, "/backend")]
#[tracing::instrument(err)]
pub async fn delete_alert_field(
    cx: Scope,
    alert_id: AlertId,
    field: AlertFieldName,
) -> Result<Alert, leptos::ServerFnError> {
    let Some(manager): Option<AlertManager> = leptos::use_context(cx) else {
        return Err(leptos::ServerFnError::ServerError("Missing manager".to_owned()));
    };

    manager
        .edit_alert(&alert_id, move |alert| {
            alert.fields.remove(&field);
        })
        .await?;

    let map_r = manager.alerts.read().await;
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

    let map_r = manager.alerts.read().await;
    let alert = map_r.get(&alert_id).expect("no alert found");
    Ok(alert.clone())
}

#[cfg(feature = "ssr")]
pub(crate) fn register_server_fns() {
    _ = UpdateAlertText::register();
    _ = UpdateAlertField::register();
    _ = DeleteAlertField::register();
    _ = AddAlertField::register();
}
