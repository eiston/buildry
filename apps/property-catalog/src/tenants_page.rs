use crate::housework_model::HouseworkTicket;
use crate::icon::Icon;
use dioxus::prelude::*;
use gloo_net::http::Request;
use serde::Deserialize;

#[derive(Clone, PartialEq, Deserialize)]
struct MyTenancy {
    id: i64,
    property_id: i64,
    property_name: String,
    room_name: String,
    start_date: String,
    end_date: String,
}

fn date_label(value: &str) -> String {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|date| date.format("%b %-d, %Y").to_string())
        .unwrap_or_else(|_| {
            if value.is_empty() {
                "Ongoing".into()
            } else {
                value.into()
            }
        })
}

#[component]
fn TenantChrome(active: &'static str, title: String, create: bool, search: bool) -> Element {
    rsx! {
        header { class: "tenant-app-header",
            div { class: "tenant-brand", img { class: "brand-symbol", src: "/icons/buildry-mark.svg?v=20261017-vibrant", alt: "" } strong { "Buildry" } }
            nav { class: "tenant-desktop-nav", aria_label: "Tenant navigation",
                a { class: if active == "home" { "active" } else { "" }, href: "/my-home", "Home" }
                a { class: if active == "requests" { "active" } else { "" }, href: "/my-home/requests", "Requests" }
                a { class: if active == "account" { "active" } else { "" }, href: "/my-home/account", "Account" }
            }
        }
        div { class: "tenant-mobile-topbar",
            h1 { "{title}" }
            if search { a { class: "tenant-top-action", href: "/my-home/requests/search", aria_label: "Search requests", title: "Search requests", Icon { name: "search" } } }
            if create { a { class: "tenant-top-action", href: "/my-home/requests/new", aria_label: "New request", title: "New request", Icon { name: "plus" } } }
        }
        nav { class: "tenant-bottom-nav", aria_label: "Tenant navigation",
            a { class: if active == "home" { "active" } else { "" }, href: "/my-home", aria_current: if active == "home" { "page" } else { "false" }, Icon { name: "house" } span { "Home" } }
            a { class: if active == "requests" { "active" } else { "" }, href: "/my-home/requests", aria_current: if active == "requests" { "page" } else { "false" }, Icon { name: "check" } span { "Requests" } }
            a { class: if active == "account" { "active" } else { "" }, href: "/my-home/account", aria_current: if active == "account" { "page" } else { "false" }, Icon { name: "users" } span { "Account" } }
        }
    }
}

pub fn my_home_page() -> Element {
    let mut tenancies = use_signal(Vec::<MyTenancy>::new);
    let mut message = use_signal(String::new);
    use_effect(move || {
        spawn(async move {
            match Request::get("/api/my-tenancies").send().await {
                Ok(response) if response.ok() => match response.json::<Vec<MyTenancy>>().await {
                    Ok(items) => tenancies.set(items),
                    Err(_) => message.set("Could not read your tenancies.".into()),
                },
                _ => message.set("Could not load your tenancies.".into()),
            }
        });
    });
    rsx! {
        document::Title { "My home | Buildry" }
        document::Stylesheet { href: crate::CSS }
        main { class: "tenant-home tenant-app-page",
            TenantChrome { active: "home", title: "Home", create: false, search: false }
            div { class: "tenant-home-content",
                h1 { class: "tenant-desktop-title", "My home" }
                p { class: "tenant-home-intro", "Your current home and tenancy details." }
            if !message().is_empty() { p { class: "availability-message", role: "status", "{message}" } }
            if tenancies().is_empty() { div { class: "tenant-requests-empty", h2 { "No confirmed lease yet" } p { "Your home will appear here once your tenancy is confirmed." } } }
            else { div { class: "tenant-home-list",
                for tenancy in tenancies() { article { key: "{tenancy.id}", class: "tenant-home-card",
                    span { class: "tenant-home-card-icon", Icon { name: "house" } }
                    div { h2 { "{tenancy.property_name}" } p { "{tenancy.room_name}" } small { "{date_label(&tenancy.start_date)} – {date_label(&tenancy.end_date)}" } }
                    a { href: "/my-home/requests/new?property_id={tenancy.property_id}", "Report an issue", Icon { name: "arrow-up-right" } }
                } }
            } }
            }
        }
    }
}

pub fn account_page() -> Element {
    let mut message = use_signal(String::new);
    let mut current_password = use_signal(String::new);
    let mut new_password = use_signal(String::new);
    let mut changing = use_signal(|| false);
    rsx! {
        document::Title { "Account | Buildry" }
        document::Stylesheet { href: crate::CSS }
        main { class: "tenant-home tenant-app-page tenant-account-page",
            TenantChrome { active: "account", title: "Account", create: false, search: false }
            div { class: "tenant-home-content",
                h1 { class: "tenant-desktop-title", "Account" }
                section { class: "tenant-panel tenant-password-panel",
                    h2 { "Change password" }
                    p { "Keep your account secure with a password of at least 12 characters." }
                    if !message().is_empty() { p { class: "tenant-request-error", role: "alert", "{message}" } }
                    crate::app_form::AppForm { class: "tenant-form", onsubmit: move |event: FormEvent| { event.prevent_default(); if changing() { return; } changing.set(true); message.set(String::new()); let body = serde_json::json!({ "current_password": current_password(), "new_password": new_password() }); spawn(async move {
                        let result = Request::post("/api/change-password").json(&body);
                        match result { Ok(request) => match request.send().await { Ok(response) if response.ok() => { let _ = web_sys::window().and_then(|window| window.location().set_href("/login").ok()); }, Ok(response) => message.set(response.text().await.unwrap_or_else(|_| "Could not change password.".into())), Err(_) => message.set("Could not change password.".into()) }, Err(error) => message.set(error.to_string()) }
                        changing.set(false);
                    }); },
                        label { "Current password *", crate::form_input::FormInput { r#type: "password", required: true, autocomplete: "current-password", value: "{current_password}", oninput: move |event: FormEvent| current_password.set(event.value()) } }
                        label { "New password *", crate::form_input::FormInput { r#type: "password", required: true, minlength: "12", autocomplete: "new-password", value: "{new_password}", oninput: move |event: FormEvent| new_password.set(event.value()) } }
                        button { class: "primary-button", r#type: "submit", disabled: changing(), if changing() { "Saving…" } else { "Change password" } }
                    }
                }
                crate::app_form::AppForm { class: "tenant-sign-out", method: "post", action: "/logout", button { r#type: "submit", "Sign out" } }
            }
        }
    }
}

fn request_status(value: &str) -> &'static str {
    match value {
        "planned" => "Scheduled",
        "in_progress" => "In progress",
        "done" => "Completed",
        "aborted" => "Closed",
        _ => "Submitted",
    }
}

fn request_date(value: &str) -> String {
    value
        .get(..10)
        .map(date_label)
        .unwrap_or_else(|| value.to_string())
}

pub fn maintenance_page() -> Element {
    let path = web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .unwrap_or_default();
    let is_new = path.ends_with("/new");
    let is_search = path.ends_with("/search");
    let detail_id = path
        .strip_prefix("/my-home/requests/")
        .and_then(|part| part.parse::<i64>().ok());
    let selected_property_id = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .and_then(|search| {
            search.trim_start_matches('?').split('&').find_map(|part| {
                part.strip_prefix("property_id=")
                    .and_then(|id| id.parse::<i64>().ok())
            })
        });
    let mut requests = use_signal(Vec::<HouseworkTicket>::new);
    let mut detail = use_signal(|| None::<HouseworkTicket>);
    let mut title = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut photos = use_signal(Vec::<String>::new);
    let mut error = use_signal(String::new);
    let mut loading = use_signal(|| true);
    let mut saving = use_signal(|| false);
    let mut uploading = use_signal(|| false);
    let mut search_term = use_signal(String::new);
    use_effect(move || {
        spawn(async move {
            let endpoint = detail_id
                .map(|id| format!("/api/my-maintenance-requests/{id}"))
                .unwrap_or_else(|| "/api/my-maintenance-requests".into());
            match Request::get(&endpoint).send().await {
                Ok(response) if response.ok() => {
                    if detail_id.is_some() {
                        match response.json::<HouseworkTicket>().await {
                            Ok(item) => detail.set(Some(item)),
                            Err(_) => error.set("Could not read this request.".into()),
                        }
                    } else if let Ok(items) = response.json::<Vec<HouseworkTicket>>().await {
                        requests.set(items);
                    } else {
                        error.set("Could not read your requests.".into());
                    }
                }
                Ok(response) => error.set(
                    response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Could not load requests.".into()),
                ),
                Err(_) => error.set("Could not connect to requests.".into()),
            }
            loading.set(false);
        });
    });
    let term = search_term().to_lowercase();
    let visible_requests: Vec<_> = requests()
        .into_iter()
        .filter(|item| {
            term.is_empty()
                || format!("{} {} {}", item.title, item.property_name, item.description)
                    .to_lowercase()
                    .contains(&term)
        })
        .collect();
    rsx! {
        document::Title { if is_new { "New maintenance request | Buildry" } else if detail_id.is_some() { "Maintenance request | Buildry" } else if is_search { "Search maintenance requests | Buildry" } else { "Maintenance requests | Buildry" } }
        document::Stylesheet { href: crate::CSS }
        main { class: "tenant-home tenant-app-page tenant-requests-page",
            TenantChrome { active: "requests", title: if is_new { "New request" } else if detail_id.is_some() { "Request" } else if is_search { "Search" } else { "Requests" }, create: !is_new && detail_id.is_none() && !is_search, search: !is_new && detail_id.is_none() && !is_search }
            nav { class: "tenant-breadcrumb", aria_label: "Breadcrumb", a { href: "/my-home", "My home" } span { " / " } if is_new || detail_id.is_some() { a { href: "/my-home/requests", "Requests" } span { " / " } span { if is_new { "New request" } else { "Details" } } } else { span { "Requests" } } }
            if is_new {
                div { class: "tenant-requests-heading", p { class: "tenant-eyebrow", "MAINTENANCE" } h1 { "Report an issue" } p { "Tell us what needs attention. Photos help us understand the problem." } }
                crate::app_form::AppForm { class: "tenant-request-form", onsubmit: move |event: FormEvent| { event.prevent_default(); if saving() || uploading() { return; } if description().trim().is_empty() { error.set("Add a description of the issue.".into()); return; } error.set(String::new()); saving.set(true); let body = serde_json::json!({"title": title(), "description": description(), "photos": photos(), "property_id": selected_property_id}); spawn(async move {
                    let result = Request::post("/api/my-maintenance-requests").json(&body);
                    match result { Ok(request) => match request.send().await { Ok(response) if response.ok() => match response.json::<HouseworkTicket>().await { Ok(item) => { if let Some(window)=web_sys::window() { let _=window.location().set_href(&format!("/my-home/requests/{}", item.id)); } }, Err(_) => error.set("Request submitted, but the details could not be opened.".into()) }, Ok(response) => error.set(response.text().await.unwrap_or_else(|_| "Could not submit your request.".into())), Err(_) => error.set("Could not submit your request.".into()) }, Err(_) => error.set("Could not prepare your request.".into()) }
                    saving.set(false);
                }); },
                    label { "Title *", crate::form_input::FormInput { required: true, maxlength: "180", placeholder: "e.g. Kitchen tap is leaking", value: "{title}", oninput: move |event: FormEvent| title.set(event.value()) } }
                    label { "Description", crate::notebook_textarea::NotebookTextarea { label: "Description", value: description(), placeholder: "Where is the issue? What happened?", maxlength: 10000, onchange: move |text| description.set(text) } }
                    div { class: "tenant-request-photos", strong { "Photos" } p { "Optional · JPEG, PNG or WebP · up to 5 MB each" }
                        label { class: "tenant-upload-button", "Add photos", crate::form_input::FormFileInput { accept: "image/jpeg,image/png,image/webp", multiple: true, disabled: uploading() || photos().len() >= 24, onchange: move |event: FormEvent| { let files=event.files(); if files.is_empty() { return; } uploading.set(true); error.set(String::new()); spawn(async move { for file in files { if photos().len()>=24 { break; } match crate::api::upload_maintenance_image(file).await { Ok(path) => { let mut next=photos(); next.push(path); photos.set(next); }, Err(message) => { error.set(message); break; } } } uploading.set(false); }); } } }
                        if uploading() { p { role: "status", "Uploading photos…" } }
                        if !photos().is_empty() { div { class: "tenant-photo-grid", for (index, photo) in photos().iter().enumerate() { div { key: "{photo}", img { src: "{photo}", alt: "Selected request photo {index + 1}" } button { r#type: "button", aria_label: "Remove photo {index + 1}", onclick: move |_| { let mut next=photos(); if index<next.len() { next.remove(index); photos.set(next); } }, "×" } } } } }
                    }
                    if !error().is_empty() { p { class: "tenant-request-error", role: "alert", "{error}" } }
                    div { class: "tenant-request-actions", a { href: "/my-home/requests", "Cancel" } button { class: "primary-button", r#type: "submit", disabled: saving() || uploading(), if saving() { "Submitting…" } else { "Submit request" } } }
                }
            } else if detail_id.is_some() {
                if loading() { p { "Loading request…" } }
                else if let Some(item) = detail() { div { class: "tenant-requests-heading", p { class: "tenant-eyebrow", "REQUEST #{item.id}" } h1 { "{item.title}" } span { class: "tenant-request-status", "{request_status(&item.status)}" } }
                    article { class: "tenant-request-detail", div { span { "Property" } strong { "{item.property_name}" } } div { span { "Submitted" } strong { "{request_date(&item.created_at)}" } } section { h2 { "Description" } p { "{item.description}" } } if !item.photos.is_empty() { section { h2 { "Photos" } div { class: "tenant-photo-grid", for (index, photo) in item.photos.iter().enumerate() { a { href: "{photo}", target: "_blank", rel: "noopener", img { src: "{photo}", alt: "Request photo {index + 1}" } } } } } } }
                } else { p { class: "tenant-request-error", role: "alert", "{error}" } }
                a { class: "tenant-back-link", href: "/my-home/requests", "← All requests" }
            } else {
                if is_search { div { class: "tenant-request-search", a { href: "/my-home/requests", aria_label: "Back to requests", Icon { name: "arrow-left" } } crate::form_input::FormInput { r#type: "search", placeholder: "Search requests or properties…", aria_label: "Search requests", value: "{search_term}", oninput: move |event: FormEvent| search_term.set(event.value()) } } }
                else { div { class: "tenant-requests-heading tenant-requests-list-head", div { h1 { "Requests" } } a { class: "tenant-desktop-new-request", href: "/my-home/requests/new", Icon { name: "plus" } "New request" } } }
                if loading() { p { "Loading requests…" } }
                else if !error().is_empty() { p { class: "tenant-request-error", role: "alert", "{error}" } }
                else if requests().is_empty() { div { class: "tenant-requests-empty", h2 { "No requests yet" } p { "Need something fixed? Send us a request and track it here." } a { class: "primary-button", href: "/my-home/requests/new", "Report an issue" } } }
                else if visible_requests.is_empty() { div { class: "tenant-requests-empty", h2 { "No matching requests" } p { "Try another search." } } }
                else { div { class: "task-joint-list tenant-task-list", div { class: "task-list-items",
                    for item in visible_requests { a { key: "{item.id}", class: "task-register-row register-row", href: "/my-home/requests/{item.id}",
                        span { class: "task-row-top", span { class: "housework-property", span { class: "housework-house-avatar", Icon { name: "house" } } "{item.property_name}" } span { class: "tenant-request-status", "{request_status(&item.status)}" } }
                        span { class: "task-row-main", span { class: "tenant-task-avatar", Icon { name: "house" } } span { class: "task-row-copy", strong { "{item.title}" } small { class: "task-row-number", "REQUEST-{item.id} · {request_date(&item.created_at)}" } } }
                    } }
                } } }
            }
        }
    }
}
