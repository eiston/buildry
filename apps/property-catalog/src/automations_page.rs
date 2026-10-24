use crate::icon::Icon;
use crate::model::Property;
use dioxus::prelude::*;
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, PartialEq, Deserialize, Serialize)]
struct Template {
    id: i64,
    key: String,
    name: String,
    description: String,
    enabled: bool,
    trigger: Value,
    conditions: Value,
    action: Value,
}

#[derive(Clone, PartialEq, Deserialize)]
struct Preset {
    template: Template,
    enabled: bool,
}

#[derive(Serialize)]
struct TemplateSave<'a> {
    key: &'a str,
    name: &'a str,
    description: &'a str,
    enabled: bool,
    trigger: &'a Value,
    conditions: &'a Value,
    action: &'a Value,
}

fn message_from(response: &gloo_net::http::Response) -> String {
    format!("Request failed ({})", response.status())
}

pub fn landlord_properties_page() -> Element {
    let mut properties = use_signal(Vec::<Property>::new);
    let mut error = use_signal(String::new);
    use_effect(move || {
        spawn(async move {
            match Request::get("/api/tasks/properties").send().await {
                Ok(response) if response.ok() => match response.json::<Vec<Property>>().await {
                    Ok(items) => properties.set(items),
                    Err(_) => error.set("Could not read properties.".into()),
                },
                Ok(response) => error.set(message_from(&response)),
                Err(_) => error.set("Could not load properties.".into()),
            }
        });
    });
    rsx! { main { class: "main-content automation-page",
        div { class: "page-head", h1 { "Properties" } p { "Select a property to manage its maintenance tasks." } }
        if !error().is_empty() { p { role: "alert", "{error}" } }
        div { class: "automation-list", for property in properties() { a { class: "automation-card", href: "/my-properties/{property.id}",
            div { strong { "{property.name}" } p { "{property.address}" } }
            Icon { name: "chevron-right" }
        } } }
    } }
}

pub fn landlord_property_page() -> Element {
    let path = web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .unwrap_or_default();
    let id = path
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .parse::<u32>()
        .unwrap_or(0);
    let mut property = use_signal(|| None::<Property>);
    use_effect(move || {
        spawn(async move {
            if let Ok(response) = Request::get("/api/tasks/properties").send().await {
                if let Ok(items) = response.json::<Vec<Property>>().await {
                    property.set(items.into_iter().find(|item| item.id == id));
                }
            }
        });
    });
    rsx! { main { class: "main-content automation-page",
        a { href: "/my-properties", "← Properties" }
        if let Some(item) = property() { div { class: "page-head", h1 { "{item.name}" } p { "{item.address}" } }
            PropertyMaintenance { property_id: id.into() }
        }
    } }
}

#[component]
pub fn PropertyMaintenance(property_id: i64) -> Element {
    let mut presets = use_signal(Vec::<Preset>::new);
    let mut error = use_signal(String::new);
    let mut busy = use_signal(|| None::<i64>);
    let mut loading = use_signal(|| true);
    use_effect(move || {
        spawn(async move {
            match Request::get(&format!("/api/properties/{property_id}/automations"))
                .send()
                .await
            {
                Ok(response) if response.ok() => match response.json::<Vec<Preset>>().await {
                    Ok(items) => presets.set(items),
                    Err(_) => error.set("Could not read maintenance presets.".into()),
                },
                Ok(response) => error.set(message_from(&response)),
                Err(_) => error.set("Could not load maintenance presets.".into()),
            }
            loading.set(false);
        });
    });
    rsx! { section { class: "property-maintenance-section",
        div { class: "property-maintenance-heading", h2 { "Automatic maintenance" } }
        div { class: "template-toggle-card property-maintenance-card",
        if !error().is_empty() { p { class: "availability-message", role: "alert", "{error}" } }
        if loading() { p { class: "property-maintenance-loading", "Loading…" } }
        else { div { class: "template-toggle-list",
            for preset in presets() { button { key: "{preset.template.id}", r#type: "button",
                class: if preset.enabled { "form-toggle on" } else { "form-toggle" },
                role: "switch", aria_checked: preset.enabled,
                disabled: busy().is_some() || !preset.template.enabled,
                onclick: { let template_id = preset.template.id; let enabled = !preset.enabled; move |_| {
                        busy.set(Some(template_id)); error.set(String::new());
                        spawn(async move {
                            let request = Request::put(&format!("/api/properties/{property_id}/automations/{template_id}"))
                                .json(&serde_json::json!({"enabled":enabled}));
                            match request {
                                Ok(request) => match request.send().await {
                                    Ok(response) if response.ok() => { let mut items = presets(); if let Some(item) = items.iter_mut().find(|item|item.template.id==template_id) { item.enabled=enabled; } presets.set(items); },
                                    Ok(response) => error.set(message_from(&response)),
                                    Err(_) => error.set("Could not save this preset.".into()),
                                },
                                Err(_) => error.set("Could not prepare this change.".into()),
                            }
                            busy.set(None);
                        });
                    } },
                span { class: "property-maintenance-copy", strong { "{preset.template.name}" } small { "{preset.template.description}" } }
                span { class: "form-toggle-track", span { class: "form-toggle-thumb" } }
            } }
        } }
        }
    } }
}

pub fn maintenance_redirect_page() -> Element {
    use_effect(move || {
        spawn(async move {
            let path = web_sys::window()
                .and_then(|w| w.location().pathname().ok())
                .unwrap_or_default();
            let property_id = path.split('/').nth(2).unwrap_or_default();
            let landlord = if let Ok(response) = Request::get("/api/me").send().await {
                response
                    .json::<Value>()
                    .await
                    .map(|user| user["role"] == "landlord")
                    .unwrap_or(false)
            } else {
                false
            };
            let destination = if landlord {
                format!("/my-properties/{property_id}")
            } else {
                format!("/properties/{property_id}")
            };
            if let Some(window) = web_sys::window() {
                let _ = window.location().replace(&destination);
            }
        });
    });
    rsx! {}
}

fn template_summary(item: &Template) -> String {
    match item.trigger["type"].as_str() {
        Some("schedule") => format!(
            "Every {} days",
            item.trigger["every_days"].as_i64().unwrap_or(30)
        ),
        Some("season") => "Seasonal dates".into(),
        Some("weather") => "Weather forecast".into(),
        Some("property_event") => "Property event".into(),
        _ => "Automation".into(),
    }
}

fn template_key(name: &str) -> String {
    let mut key = String::new();
    for character in name.to_ascii_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            key.push(character);
        } else if !key.is_empty() && !key.ends_with('-') {
            key.push('-');
        }
    }
    key.trim_end_matches('-').to_string()
}

fn season_dates(first: String, second: String) -> Value {
    serde_json::json!([first, second]
        .into_iter()
        .filter(|date| !date.is_empty())
        .collect::<Vec<_>>())
}

pub fn admin_page() -> Element {
    let mut templates = use_signal(Vec::<Template>::new);
    let mut selected = use_signal(|| None::<i64>);
    let mut draft = use_signal(|| None::<Template>);
    let mut message = use_signal(String::new);
    let mut busy = use_signal(|| false);
    use_effect(move || {
        spawn(async move {
            match Request::get("/api/automations/templates").send().await {
                Ok(response) if response.ok() => match response.json::<Vec<Template>>().await {
                    Ok(items) => {
                        if let Some(first) = items.first() {
                            selected.set(Some(first.id));
                            draft.set(Some(first.clone()));
                        }
                        templates.set(items);
                    }
                    Err(_) => message.set("Could not read templates.".into()),
                },
                Ok(response) => message.set(message_from(&response)),
                Err(_) => message.set("Could not load templates.".into()),
            }
        });
    });
    rsx! { main { class: "main-content automation-admin-page",
        header { class: "automation-page-header", div { span { class: "automation-eyebrow", "BUILDRY / SETTINGS" } h1 { "Automations" } p { "Set up the tasks Buildry creates for each property." } } }
        if !message().is_empty() { p { class: "availability-message", role: "status", "{message}" } }
        div { class: "automation-workspace",
            aside { class: "automation-template-panel",
                div { class: "automation-template-head", h2 { "Templates" } button { r#type: "button", class: "automation-add", onclick: move |_| {
                    let item = Template { id: 0, key: String::new(), name: String::new(), description: String::new(), enabled: true, trigger: serde_json::json!({"type":"schedule","every_days":30}), conditions: serde_json::json!({}), action: serde_json::json!({"type":"create_task","title":"","description":""}) };
                    selected.set(Some(0)); draft.set(Some(item)); message.set(String::new());
                }, Icon { name: "plus" } "New" } }
                div { class: "automation-template-items",
                    for item in templates() { button { key: "{item.id}", r#type: "button", class: if selected()==Some(item.id) { "automation-template-item active" } else { "automation-template-item" }, onclick: { let chosen = item.clone(); move |_| { selected.set(Some(chosen.id)); draft.set(Some(chosen.clone())); message.set(String::new()); } },
                        span { class: "automation-template-name", "{item.name}" }
                        span { class: "automation-template-meta", "{template_summary(&item)}" span { class: if item.enabled { "automation-state on" } else { "automation-state" }, if item.enabled { "On" } else { "Off" } } }
                    } }
                }
            }
            if let Some(item) = draft() { crate::app_form::AppForm { class: "automation-form", onsubmit: move |event: FormEvent| {
                event.prevent_default(); if busy() { return; }
                let Some(mut current) = draft() else { return; };
                if current.id==0 { current.key = template_key(&current.name); }
                if current.name.trim().is_empty() || current.action["title"].as_str().unwrap_or("").trim().is_empty() { message.set("Add a template name and task title.".into()); return; }
                busy.set(true); message.set(String::new());
                spawn(async move {
                    let body = TemplateSave { key: &current.key, name: &current.name, description: &current.description, enabled: current.enabled, trigger: &current.trigger, conditions: &current.conditions, action: &current.action };
                    let request = if current.id==0 { Request::post("/api/automations/templates") } else { Request::put(&format!("/api/automations/templates/{}",current.id)) };
                    match request.json(&body) {
                        Ok(request) => match request.send().await {
                            Ok(response) if response.ok() => match response.json::<Template>().await { Ok(saved) => { let mut items = templates(); if let Some(item)=items.iter_mut().find(|item|item.id==saved.id){*item=saved.clone();} else { items.push(saved.clone()); } templates.set(items); selected.set(Some(saved.id)); draft.set(Some(saved)); message.set("Template saved.".into()); }, Err(_) => message.set("Could not read the saved template.".into()) },
                            Ok(response) => message.set(format!("Could not save template ({})", response.status())), Err(_) => message.set("Could not save template.".into()),
                        }, Err(_) => message.set("Could not prepare template.".into()),
                    }
                    busy.set(false);
                });
            },
                div { class: "automation-form-top", div { span { class: "automation-eyebrow", if item.id==0 { "NEW TEMPLATE" } else { "EDIT TEMPLATE" } } h2 { if item.name.is_empty() { "Untitled automation" } else { "{item.name}" } } } span { class: if item.enabled { "automation-state on" } else { "automation-state" }, if item.enabled { "Active" } else { "Paused" } } }
                section { class: "automation-form-section",
                    h3 { "Template" }
                    div { class: "automation-fields", label { "Name *", crate::form_input::FormInput { required: true, maxlength: "120", value: "{item.name}", placeholder: "e.g. Lawn care", oninput: move |e: FormEvent| { if let Some(ref mut item)=*draft.write() { item.name=e.value(); } } } }
                        label { class: "wide", "Summary", crate::form_input::FormInput { maxlength: "1000", value: "{item.description}", placeholder: "Short description for administrators", oninput: move |e: FormEvent| { if let Some(ref mut item)=*draft.write() { item.description=e.value(); } } } }
                    }
                    button { r#type: "button", class: if item.enabled { "form-toggle on automation-enabled" } else { "form-toggle automation-enabled" }, role: "switch", aria_checked: item.enabled, onclick: move |_| { if let Some(ref mut item)=*draft.write() { item.enabled=!item.enabled; } }, span { "Template active" } span { class: "form-toggle-track", span { class: "form-toggle-thumb" } } }
                }
                section { class: "automation-form-section",
                    h3 { "When to create a task" }
                    div { class: "automation-fields", label { class: "wide", "Trigger", crate::form_input::FormNativeSelect { value: item.trigger["type"].as_str().unwrap_or("schedule"), onchange: move |e: FormEvent| { if let Some(ref mut item)=*draft.write() { item.trigger=match e.value().as_str() { "season" => serde_json::json!({"type":"season","month_days":["04-15","10-15"]}), "weather" => serde_json::json!({"type":"weather","metric":"snowfall_sum","min":2}), "property_event" => serde_json::json!({"type":"property_event","event":"property_created"}), _ => serde_json::json!({"type":"schedule","every_days":30}) }; } },
                        option { value: "schedule", "Recurring schedule" } option { value: "season", "Dates each year" } option { value: "weather", "Weather forecast" } option { value: "property_event", "Property event" }
                    } } }
                    if item.trigger["type"]=="schedule" { div { class: "automation-fields", label { "Repeat every (days)", crate::form_input::FormInput { r#type: "number", min: "1", max: "3650", value: item.trigger["every_days"].as_i64().unwrap_or(30).to_string(), oninput: move |e: FormEvent| { if let Some(ref mut item)=*draft.write() { item.trigger["every_days"]=serde_json::json!(e.value().parse::<i64>().unwrap_or(0)); } } } } }
                        div { class: "automation-months", span { "Active months" } div { for (month,label) in [(1,"Jan"),(2,"Feb"),(3,"Mar"),(4,"Apr"),(5,"May"),(6,"Jun"),(7,"Jul"),(8,"Aug"),(9,"Sep"),(10,"Oct"),(11,"Nov"),(12,"Dec")] { button { r#type: "button", class: if item.trigger["months"].as_array().is_none_or(|months|months.iter().any(|value|value.as_i64()==Some(month))) { "selected" } else { "" }, onclick: move |_| { if let Some(ref mut item)=*draft.write() { let mut months:Vec<i64>=item.trigger["months"].as_array().map(|values|values.iter().filter_map(Value::as_i64).collect()).unwrap_or_else(||(1..=12).collect()); if months.contains(&month) { months.retain(|value|*value!=month); } else { months.push(month); months.sort(); } item.trigger["months"]=serde_json::json!(months); } }, "{label}" } } } }
                    }
                    if item.trigger["type"]=="season" { div { class: "automation-fields", label { "First date (MM-DD)", crate::form_input::FormInput { placeholder: "04-15", value: item.trigger["month_days"].as_array().and_then(|days|days.first()).and_then(Value::as_str).unwrap_or("").to_string(), oninput: move |e: FormEvent| { if let Some(ref mut item)=*draft.write() { let second=item.trigger["month_days"][1].as_str().unwrap_or("").to_string(); item.trigger["month_days"]=season_dates(e.value(),second); } } } }
                        label { "Second date (MM-DD)", crate::form_input::FormInput { placeholder: "10-15", value: item.trigger["month_days"].as_array().and_then(|days|days.get(1)).and_then(Value::as_str).unwrap_or("").to_string(), oninput: move |e: FormEvent| { if let Some(ref mut item)=*draft.write() { let first=item.trigger["month_days"][0].as_str().unwrap_or("").to_string(); item.trigger["month_days"]=season_dates(first,e.value()); } } } }
                    } }
                    if item.trigger["type"]=="weather" { div { class: "automation-fields", label { class: "wide", "Forecast measure", crate::form_input::FormNativeSelect { value: item.trigger["metric"].as_str().unwrap_or("snowfall_sum"), onchange: move |e: FormEvent| { if let Some(ref mut item)=*draft.write() { item.trigger["metric"]=serde_json::json!(e.value()); } }, option { value: "snowfall_sum", "Snowfall (cm)" } option { value: "rain_sum", "Rainfall (mm)" } option { value: "temperature_2m_min", "Lowest temperature (°C)" } } }
                        label { "At least", crate::form_input::FormInput { r#type: "number", step: "any", value: item.trigger["min"].as_f64().map(|value|value.to_string()).unwrap_or_default(), oninput: move |e: FormEvent| { if let Some(ref mut item)=*draft.write() { if let Ok(value)=e.value().parse::<f64>() { item.trigger["min"]=serde_json::json!(value); } else { item.trigger.as_object_mut().unwrap().remove("min"); } } } } }
                        label { "At most", crate::form_input::FormInput { r#type: "number", step: "any", value: item.trigger["max"].as_f64().map(|value|value.to_string()).unwrap_or_default(), oninput: move |e: FormEvent| { if let Some(ref mut item)=*draft.write() { if let Ok(value)=e.value().parse::<f64>() { item.trigger["max"]=serde_json::json!(value); } else { item.trigger.as_object_mut().unwrap().remove("max"); } } } } }
                    } }
                    if item.trigger["type"]=="property_event" { div { class: "automation-fields", label { class: "wide", "Event name", crate::form_input::FormInput { value: item.trigger["event"].as_str().unwrap_or(""), placeholder: "property_created", oninput: move |e: FormEvent| { if let Some(ref mut item)=*draft.write() { item.trigger["event"]=serde_json::json!(e.value()); } } } } } }
                }
                section { class: "automation-form-section", h3 { "Only when" }
                    button { r#type: "button", class: if item.conditions["requires_coordinates"]==true { "form-toggle on automation-enabled" } else { "form-toggle automation-enabled" }, role: "switch", aria_checked: item.conditions["requires_coordinates"]==true, onclick: move |_| { if let Some(ref mut item)=*draft.write() { item.conditions["requires_coordinates"]=serde_json::json!(item.conditions["requires_coordinates"]!=true); } }, span { "Property has a location" } span { class: "form-toggle-track", span { class: "form-toggle-thumb" } } }
                }
                section { class: "automation-form-section", h3 { "Task to create" }
                    div { class: "automation-fields", label { class: "wide", "Task title *", crate::form_input::FormInput { required: true, maxlength: "180", value: item.action["title"].as_str().unwrap_or(""), placeholder: "e.g. Clear snow and ice", oninput: move |e: FormEvent| { if let Some(ref mut item)=*draft.write() { item.action["title"]=serde_json::json!(e.value()); } } } }
                        label { class: "wide", "Task instructions", crate::notebook_textarea::NotebookTextarea { label: "Task instructions", value: item.action["description"].as_str().unwrap_or("").to_string(), placeholder: "What should the assigned person do?", maxlength: 10000, onchange: move |text| { if let Some(ref mut item)=*draft.write() { item.action["description"]=serde_json::json!(text); } } } }
                    }
                }
                div { class: "automation-form-footer", button { class: "primary-button", r#type: "submit", disabled: busy(), Icon { name: "check" } if busy() { "Saving…" } else { "Save template" } } }
            } }
        }
    } }
}
