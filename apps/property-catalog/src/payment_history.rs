use crate::{form_select::FormSelect, icon::Icon, searchable_list::JointList};
use dioxus::prelude::*;
use gloo_net::http::Request;
use serde::Deserialize;

pub fn selected_lease() -> Option<i64> {
    web_sys::window()?
        .location()
        .search()
        .ok()?
        .trim_start_matches('?')
        .split('&')
        .find_map(|part| part.strip_prefix("lease=")?.parse().ok())
}

#[component]
pub fn PaymentLeaseSelect(
    selected: i64,
    choices: Vec<(String, String)>,
    path: &'static str,
) -> Element {
    rsx! {
        FormSelect { label: "Lease", value: selected.to_string(), required: false,
            placeholder: "Select a lease", options: choices, onchange: move |value: String| {
                if let Ok(id) = value.parse::<i64>() {
                    if let Some(window) = web_sys::window() {
                        let _ = window.location().set_href(&format!("{path}?lease={id}"));
                    }
                }
            }
        }
    }
}

#[derive(Clone, Deserialize)]
struct PaymentRecord {
    id: String,
    kind: String,
    date: String,
    amount_cents: u64,
    status: String,
    invoice_url: Option<String>,
}

#[derive(Clone, Deserialize)]
struct History {
    rows: Vec<PaymentRecord>,
    test_mode: bool,
    stripe_unavailable: bool,
}

#[component]
pub fn PaymentHistoryList(endpoint: String) -> Element {
    let mut history = use_signal(|| None::<History>);
    let mut failed = use_signal(|| false);
    use_effect(move || {
        let endpoint = endpoint.clone();
        spawn(async move {
            if let Ok(response) = Request::get(&endpoint).send().await {
                if response.ok() {
                    if let Ok(value) = response.json::<History>().await {
                        history.set(Some(value));
                        return;
                    }
                }
            }
            failed.set(true);
        });
    });
    rsx! {
        if let Some(history) = history() {
            if history.test_mode { span { class: "rent-badge payment-history-mode", "Test mode" } }
            if history.stripe_unavailable { p { class: "tenant-request-error", role: "alert", "Some Stripe payments could not be loaded." } }
            JointList { class: "payment-history-list",
                if history.rows.is_empty() {
                    p { class: "payment-history-message", "No payment history yet" }
                }
                for row in history.rows {
                    div { key: "{row.id}", class: "register-row payment-history-row",
                        span { class: "payment-history-icon", aria_hidden: "true", Icon { name: "file" } }
                        div { class: "payment-history-copy",
                            strong { match row.kind.as_str() {
                                "rent_deposit" => "Last month’s rent deposit",
                                "key_deposit" => "Refundable key deposit",
                                _ => "Monthly rent",
                            } }
                            small { {chrono::NaiveDate::parse_from_str(&row.date, "%Y-%m-%d")
                                .map(|date| date.format("%b %-d, %Y").to_string()).unwrap_or(row.date)} }
                        }
                        div { class: "payment-history-value",
                            strong { {format!("C${}.{:02}", row.amount_cents / 100, row.amount_cents % 100)} }
                            span { class: if matches!(row.status.as_str(), "paid" | "recorded") { "rent-badge rent-badge-active" } else { "rent-badge" },
                                match row.status.as_str() {
                                    "paid" => "Paid",
                                    "recorded" => "Received",
                                    "open" => "Open",
                                    "attention" => "Payment needs attention",
                                    "processing" => "Processing",
                                    "uncollectible" => "Uncollectible",
                                    _ => "Void",
                                }
                            }
                        }
                        if let Some(url) = row.invoice_url {
                            a { class: "payment-history-invoice", href: url, target: "_blank", rel: "noopener noreferrer", "Invoice PDF", Icon { name: "arrow-up-right" } }
                        }
                    }
                }
            }
        } else if failed() {
            p { class: "tenant-request-error", role: "alert", "Could not load payment history." }
        } else {
            p { class: "payment-history-message", role: "status", "Loading payments…" }
        }
    }
}
