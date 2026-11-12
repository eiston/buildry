use crate::components::forms::form_button::{ButtonVariant, FormButton};
use crate::housework_model::HouseworkTicket;
use crate::components::icon::Icon;
use dioxus::prelude::*;
use gloo_net::http::Request;
use serde::Deserialize;

#[derive(Clone, PartialEq, Deserialize)]
pub(crate) struct MyTenancy {
    id: i64,
    property_id: i64,
    property_name: String,
    room_name: String,
    start_date: String,
    end_date: String,
    #[serde(default)]
    deposits: Vec<TenantDeposit>,
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
pub fn TenantChrome(active: &'static str, title: String, create: bool, search: bool) -> Element {
    let _ = active;
    rsx! { crate::components::forms::form_action_bar::FormActionBar {
        back_href: (if create { "/my-rentals" } else { "/my-home/requests" }).to_string(),
        back_label: (if create { "Back to rentals" } else { "Back to requests" }).to_string(),
        title: Some(title),
        if search { a { class: "header-search-action", href: "/my-home/requests/search", aria_label: "Search requests", title: "Search requests", Icon { name: "search" } } }
        if create { a { class: "round-add-action", href: "/my-home/requests/new", aria_label: "New request", title: "New request", Icon { name: "plus" } } }
    } }
}

#[derive(Clone, Deserialize)]
struct RentBilling {
    state: String,
    can_pad: bool,
    can_card: bool,
    method: Option<String>,
    bank_last4: Option<String>,
    test_mode: bool,
    amount_cents: Option<u32>,
    first_charge: Option<String>,
    next_charge: Option<String>,
    can_enroll: bool,
    can_manage: bool,
    can_stop: bool,
}

#[derive(Deserialize)]
struct BillingAction {
    url: Option<String>,
    elements: Option<CheckoutElements>,
}

#[derive(Clone, Deserialize)]
struct CheckoutElements {
    #[serde(default)]
    kind: String,
    client_secret: String,
    publishable_key: String,
    stripe_account: Option<String>,
}

#[derive(Clone, PartialEq, Deserialize)]
pub(crate) struct TenantDeposit {
    kind: String,
    amount_cents: u32,
    received_on: Option<String>,
}

fn cad(cents: u32) -> String {
    format!("${}.{:02}", cents / 100, cents % 100)
}

#[component]
pub(crate) fn RentPayments(lease_id: i64, deposits: Vec<TenantDeposit>) -> Element {
    let mut loading = use_signal(|| true);
    let mut checkout = use_signal(|| None::<CheckoutElements>);
    let mut payment_method = use_signal(|| "card".to_string());
    let mut billing = use_signal(|| None::<RentBilling>);
    let mut error = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut confirm_stop = use_signal(|| false);
    let mut refresh = use_signal(|| 0u32);
    use_effect(move || {
        refresh();
        spawn(async move {
            let endpoint = format!("/api/my-tenancies/{lease_id}/billing");
            let result = match Request::get(&endpoint).send().await {
                Ok(response) if response.ok() => response.json::<Option<RentBilling>>().await.ok(),
                _ => None,
            };
            loading.set(false);
            if let Some(value) = result {
                billing.set(value);
            } else {
                error.set("Rent payments are temporarily unavailable.".into());
            }
        });
    });
    let mut act = move |action: &'static str| {
        if busy() {
            return;
        }
        busy.set(true);
        confirm_stop.set(false);
        error.set(String::new());
        spawn(async move {
            let endpoint = format!("/api/my-tenancies/{lease_id}/billing");
            if let Ok(request) = Request::post(&endpoint).json(&action) {
                match request.send().await {
                    Ok(response) if response.ok() => {
                        if let Ok(result) = response.json::<BillingAction>().await {
                            if let Some(elements) = result.elements {
                                checkout.set(Some(elements));
                                busy.set(false);
                                return;
                            }
                            if let Some(url) = result.url {
                                if web_sys::window()
                                    .is_some_and(|window| window.location().set_href(&url).is_ok())
                                {
                                    return;
                                }
                            } else {
                                refresh += 1;
                                busy.set(false);
                                return;
                            }
                        }
                    }
                    Ok(response) => {
                        error.set(
                            response
                                .text()
                                .await
                                .unwrap_or_else(|_| "Could not update autopay.".into()),
                        );
                        busy.set(false);
                        return;
                    }
                    Err(_) => {}
                }
            }
            error.set("Could not update autopay.".into());
            busy.set(false);
        });
    };
    let details = billing();
    rsx! {
        section { class: "tenant-rent-payments", aria_label: "Rent",
            if let Some(details) = details.as_ref() {
                    div { class: "rent-payment-top",
                        div {
                            span { class: "rent-label", "Monthly rent" }
                            if let Some(amount) = details.amount_cents {
                                div { class: "rent-price", strong { "{cad(amount)}" } span { " CAD" } }
                            }
                        }
                        div { class: "rent-payment-status",
                        if details.test_mode { span { class: "rent-badge", "Test mode" } }
                        span { class: if details.state == "active" { "rent-badge rent-badge-active" } else { "rent-badge" },
                            match details.state.as_str() {
                                "active" => "Autopay on",
                                "attention" => "Payment needs attention",
                                "pending" => "Setup pending",
                                "verifying" => "Verify bank account",
                                "processing" => "Payment processing",
                                "paused" => "Autopay closed",
                                _ if checkout().is_some() => "Setup pending",
                                _ => "Autopay off",
                            }
                        }
                        }
                    }
                if let Some(elements) = checkout() {
                    document::Script { src: "/assets/rent-checkout.mjs?v=20261106-payments", r#type: "module" }
                    div { class: "rent-checkout-grid",
                        section { class: "rent-charge-card", aria_label: "Charge summary",
                            h2 { "Charge summary" }
                            dl { class: "rent-charge-summary",
                                div { dt { "Due today" } dd { class: "rent-checkout-due", if elements.kind.starts_with("pad") { "$0.00" } else { "—" } } }
                                if let Some(first_charge) = details.first_charge.as_ref() {
                                    div { dt { if elements.kind.starts_with("pad") { "First debit" } else { "First charge" } } dd { "{first_charge}" } }
                                }
                                div { dt { "GST/HST" } dd { "Exempt" } }
                            }
                        }
                        if elements.kind.starts_with("pad") {
                            crate::pages::rent_pad::RentPad { client_secret: elements.client_secret.clone(), publishable_key: elements.publishable_key.clone(), stripe_account: elements.stripe_account.clone(), verify: elements.kind == "pad_verify", on_back: move |_| { checkout.set(None); refresh += 1; } }
                        } else {
                        section { class: "rent-card-entry rent-checkout", "data-rent-checkout": "{elements.client_secret}", "data-stripe-key": "{elements.publishable_key}", "data-stripe-account": elements.stripe_account, "data-lease": "{lease_id}",
                            h2 { "Set up rent autopay" }
                            div { class: "rent-stripe-fields" }
                            p { class: "rent-checkout-loading", role: "status", "Loading secure card fields…" }
                            p { class: "rent-checkout-error", role: "alert", hidden: true }
                            div { class: "rent-checkout-actions",
                                FormButton { r#type: "button", class: "rent-checkout-submit".to_string(), variant: ButtonVariant::Primary, disabled: true, "Turn on autopay" }
                                FormButton { r#type: "button", class: "rent-checkout-back".to_string(), variant: ButtonVariant::Text, onclick: move |_| { checkout.set(None); refresh += 1; }, "Back" }
                            }
                        }
                    }
                        }
                } else {
                        if details.method.as_deref() == Some("acss_debit") && details.state != "off" { p { class: "rent-next-date", "Bank debit (PAD)" if let Some(last4) = details.bank_last4.as_ref() { " · •••• {last4}" } } }
                        if details.state == "verifying" { p { class: "rent-next-date", "Verify the two small deposits before your first scheduled debit." } }
                        if let Some(next_charge) = details.next_charge.as_ref() {
                            p { class: "rent-next-date", "Next scheduled charge {next_charge}" }
                        }
                        if details.can_enroll {
                            if let Some(first_charge) = details.first_charge.as_ref() {
                                p { class: "rent-next-date", "First autopay charge {first_charge}" }
                            }
                        }
                        if details.can_enroll && !matches!(details.state.as_str(), "pending" | "verifying") {
                            crate::components::forms::form_select::FormSelect { label: "Payment method", required: true,
                                value: if !details.can_card && details.can_pad { "pad".to_string() } else { payment_method() }, placeholder: "Choose payment method",
                                options: vec![("card".into(),"Card".into()),("pad".into(),"Bank debit (PAD)".into())],
                                disabled_values: [("card",details.can_card),("pad",details.can_pad)].into_iter().filter(|(_,available)| !available).map(|(key,_)|key.to_string()).collect::<Vec<_>>(),
                                onchange: move |value| payment_method.set(value)
                            }
                        }
                        div { class: "tenant-rent-actions",
                            if confirm_stop() {
                                FormButton { variant: ButtonVariant::Outline, onclick: move |_| confirm_stop.set(false), "Back" }
                                FormButton { variant: ButtonVariant::Danger, busy: busy(), onclick: move |_| act("stop"),
                                    if matches!(details.state.as_str(), "pending" | "verifying") { "Confirm cancel setup" } else { "Confirm turn off autopay" }
                                }
                            } else {
                                if details.can_enroll {
                                    FormButton { busy: busy(), disabled: !matches!(details.state.as_str(),"pending"|"verifying") && !details.can_card && !details.can_pad, onclick: { let resume = matches!(details.state.as_str(),"pending"|"verifying"); let only_pad = !details.can_card && details.can_pad; move |_| act(if !resume && (only_pad || payment_method() == "pad") { "enroll_pad" } else { "enroll" }) },
                                        if details.state == "verifying" { "Verify bank account" } else if details.state == "pending" { "Continue setup" } else { "Set up autopay" }
                                    }
                                }
                                if details.can_manage {
                                    FormButton { variant: ButtonVariant::Outline, busy: busy(), onclick: move |_| act("manage"), "Payment method & invoices" }
                                }
                                if details.can_stop {
                                    FormButton { variant: ButtonVariant::Text, disabled: busy(), onclick: move |_| confirm_stop.set(true),
                                        if matches!(details.state.as_str(), "pending" | "verifying") { "Cancel setup" } else { "Turn off autopay" }
                                    }
                                }
                            }
                        }
                }
            } else if loading() {
                p { class: "payment-history-message", role: "status", "Loading payments…" }
            } else if error().is_empty() && deposits.is_empty() {
                p { class: "payment-history-message", "Autopay unavailable" }
            }
            if !deposits.is_empty() {
                div { class: "rent-deposit-list",
                    h2 { "Lease deposits" }
                    for deposit in deposits {
                        div { class: "rent-deposit-summary", key: "{deposit.kind}",
                            div { strong { if deposit.kind == "rent" { "Last month’s rent" } else { "Refundable key deposit" } }
                                span { if let Some(date) = deposit.received_on.as_ref() { "Received {date_label(date)}" } else { "Receipt not recorded" } }
                            }
                            strong { "{cad(deposit.amount_cents)}" }
                        }
                    }
                }
            }
            if !error().is_empty() { p { class: "tenant-request-error", role: "alert", "{error}" } }
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
            if is_search {
                crate::components::shared_ui::PageHeader { class: "search-screen-bar",
                    a { class: "back-icon-button", href: "/my-home/requests", aria_label: "Back to requests", title: "Back to requests", Icon { name: "arrow-left" } }
                    crate::components::forms::form_input::FormInput { r#type: "search", placeholder: "Search requests or properties…", aria_label: "Search requests", autofocus: true, value: "{search_term}", oninput: move |event: FormEvent| search_term.set(event.value()) }
                }
            } else {
                TenantChrome { active: "requests", title: if is_new { "Report an issue".to_string() } else if let Some(item) = detail() { item.title } else { "Requests".to_string() }, create: !is_new && detail_id.is_none(), search: !is_new && detail_id.is_none() }
            }
            if is_new {
                div { class: "tenant-requests-heading", p { class: "tenant-eyebrow", "MAINTENANCE" } h1 { "Report an issue" } }
                crate::components::forms::app_form::AppForm { class: "tenant-request-form", onsubmit: move |event: FormEvent| { event.prevent_default(); if saving() || uploading() { return; } if description().trim().is_empty() { error.set("Add a description of the issue.".into()); return; } error.set(String::new()); saving.set(true); let body = serde_json::json!({"title": title(), "description": description(), "photos": photos(), "property_id": selected_property_id}); spawn(async move {
                    let result = Request::post("/api/my-maintenance-requests").json(&body);
                    match result { Ok(request) => match request.send().await { Ok(response) if response.ok() => match response.json::<HouseworkTicket>().await { Ok(item) => { if let Some(window)=web_sys::window() { let _=window.location().set_href(&format!("/my-home/requests/{}", item.id)); } }, Err(_) => error.set("Request submitted, but the details could not be opened.".into()) }, Ok(response) => error.set(response.text().await.unwrap_or_else(|_| "Could not submit your request.".into())), Err(_) => error.set("Could not submit your request.".into()) }, Err(_) => error.set("Could not prepare your request.".into()) }
                    saving.set(false);
                }); },
                    label { "Title *", crate::components::forms::form_input::FormInput { required: true, maxlength: "180", placeholder: "e.g. Kitchen tap is leaking", value: "{title}", oninput: move |event: FormEvent| title.set(event.value()) } }
                    label { "Description", crate::components::forms::notebook_textarea::NotebookTextarea { label: "Description", value: description(), placeholder: "Where is the issue? What happened?", maxlength: 10000, onchange: move |text| description.set(text) } }
                    div { class: "tenant-request-photos", strong { "Photos" }
                        crate::components::forms::upload_dropzone::UploadDropzone { disabled: uploading() || photos().len() >= 24,
                            on_files: move |files: Vec<dioxus::html::FileData>| {
                                if uploading() || files.is_empty() { return; }
                                uploading.set(true); error.set(String::new());
                                spawn(async move {
                                    for file in files {
                                        if photos().len() >= 24 { error.set("Maximum 24 photos.".into()); break; }
                                        let name = file.name();
                                        match crate::services::api::upload_maintenance_image(file).await {
                                            Ok(path) => photos.write().push(path),
                                            Err(message) => error.set(format!("{name}: {message}")),
                                        }
                                    }
                                    uploading.set(false);
                                });
                            }
                        }
                        if uploading() { p { role: "status", "Uploading photos…" } }
                        if !photos().is_empty() { div { class: "tenant-photo-grid", for (index, photo) in photos().iter().enumerate() { div { key: "{photo}", img { src: "{photo}", alt: "Selected request photo {index + 1}" } button { r#type: "button", aria_label: "Remove photo {index + 1}", onclick: move |_| { let mut next=photos(); if index<next.len() { next.remove(index); photos.set(next); } }, "×" } } } } }
                    }
                    if !error().is_empty() { p { class: "tenant-request-error", role: "alert", "{error}" } }
                    div { class: "tenant-request-actions", a { href: "/my-home/requests", "Cancel" } crate::components::forms::form_button::FormButton { r#type: "submit", disabled: saving() || uploading(), if saving() { "Submitting…" } else { "Submit request" } } }
                }
            } else if detail_id.is_some() {
                if loading() { p { "Loading request…" } }
                else if let Some(item) = detail() { div { class: "tenant-requests-heading", p { class: "tenant-eyebrow", "REQUEST #{item.id}" } h1 { "{item.title}" } span { class: "tenant-request-status", "{request_status(&item.status)}" } }
                    article { class: "tenant-request-detail", div { span { "Property" } strong { "{item.property_name}" } } div { span { "Submitted" } strong { "{request_date(&item.created_at)}" } } section { h2 { "Description" } p { "{item.description}" } } if !item.photos.is_empty() { section { h2 { "Photos" } div { class: "tenant-photo-grid", for (index, photo) in item.photos.iter().enumerate() { a { href: "{photo}", target: "_blank", rel: "noopener", img { src: "{photo}", alt: "Request photo {index + 1}" } } } } } } }
                } else { p { class: "tenant-request-error", role: "alert", "{error}" } }
                a { class: "tenant-back-link", href: "/my-home/requests", "← All requests" }
            } else {
                if !is_search { div { class: "tenant-requests-heading tenant-requests-list-head", h1 { "Requests" } } }
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

#[component]
pub fn AutopaySettings() -> Element {
    let mut leases=use_signal(||None::<Vec<MyTenancy>>);
    use_effect(move || {spawn(async move {if let Ok(response)=Request::get("/api/my-tenancies").send().await {if let Ok(value)=response.json::<Vec<MyTenancy>>().await {leases.set(Some(value));}}});});
    rsx! {
        if let Some(items)=leases() {
            if items.is_empty() { p { class:"payment-history-message", "No rent schedules" } }
            for lease in items.into_iter().filter(|lease|crate::pages::payment_history::selected_lease().is_none_or(|id|id==lease.id)) {
                section { class:"payment-autopay-setting", h2 { "{lease.property_name} · {lease.room_name}" }
                    RentPayments { lease_id:lease.id,deposits:lease.deposits }
                }
            }
        } else { p {role:"status", "Loading rent schedules…"} }
    }
}
