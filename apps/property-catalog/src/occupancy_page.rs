use crate::icon::Icon;
use crate::{api, model::Property};
use dioxus::prelude::*;
use gloo_net::http::Request;
use serde::Deserialize;

#[derive(Clone, PartialEq, Deserialize, Default)]
struct StayDetails {
    start_date: String,
    end_date: String,
    monthly_rent: String,
}

#[derive(Clone, PartialEq, Deserialize)]
struct OccupancyLease {
    id: i64,
    tenant_name: String,
    tenant_email: String,
    property_id: i64,
    room_id: i64,
    lease_type: String,
    details: StayDetails,
    status: String,
    signed_at: Option<String>,
    landlord_signed_at: Option<String>,
    confirmed_at: Option<String>,
    copy_sent_at: Option<String>,
    #[serde(default)] photo_id_uploaded: bool,
    #[serde(default)] photo_id_verified: bool,
}

#[derive(Clone, PartialEq, Deserialize)]
struct Account {
    id: i64,
    email: String,
    role: String,
}

fn date_label(value: &str) -> String {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|date| date.format("%b %-d, %Y").to_string())
        .unwrap_or_else(|_| if value.is_empty() { "Ongoing".into() } else { value.into() })
}

fn room_name(houses: &[Property], lease: &OccupancyLease) -> String {
    if lease.lease_type == "entire_house" { return "Entire house".into(); }
    houses.iter().find(|house| house.id as i64 == lease.property_id)
        .and_then(|house| house.spaces.iter().find(|room| room.id as i64 == lease.room_id))
        .map(|room| room.name.clone()).unwrap_or_else(|| "Room unavailable".into())
}

fn property_name(houses: &[Property], lease: &OccupancyLease) -> String {
    houses.iter().find(|house| house.id as i64 == lease.property_id)
        .map(|house| house.name.clone()).unwrap_or_else(|| "Property unavailable".into())
}

#[component]
pub fn OccupancyPage() -> Element {
    let mut leases = use_signal(Vec::<OccupancyLease>::new);
    let mut houses = use_signal(Vec::<Property>::new);
    let mut accounts = use_signal(Vec::<Account>::new);
    let mut selected_id = use_signal(|| 0_i64);
    let mut filter = use_signal(|| "all".to_string());
    let mut search = use_signal(String::new);
    let mut message = use_signal(String::new);
    let mut loading = use_signal(|| true);
    let mut busy = use_signal(|| false);

    use_effect(move || { spawn(async move {
        match Request::get("/api/leases").send().await {
            Ok(response) if response.ok() => match response.json::<Vec<OccupancyLease>>().await {
                Ok(items) => {
                    selected_id.set(items.iter().find(|lease| lease.status == "signed" || lease.status == "confirmed").map(|lease| lease.id).unwrap_or(0));
                    leases.set(items);
                }
                Err(_) => message.set("Could not read leases.".into()),
            },
            _ => message.set("Could not load leases.".into()),
        }
        if let Ok(items) = api::list().await { houses.set(items); }
        if let Ok(response) = Request::get("/api/access/users").send().await {
            if let Ok(items) = response.json::<Vec<Account>>().await { accounts.set(items); }
        }
        loading.set(false);
    }); });

    let all: Vec<_> = leases().into_iter().filter(|lease| lease.status == "signed" || lease.status == "confirmed").collect();
    let signed = all.iter().filter(|lease| lease.status == "signed").count();
    let confirmed = all.iter().filter(|lease| lease.status == "confirmed").count();
    let properties = houses();
    let term = search().to_lowercase();
    let visible: Vec<_> = all.iter().filter(|lease| {
        (filter() == "all" || lease.status == filter()) &&
        (term.is_empty() || format!("{} {} {} {}", lease.tenant_name, lease.tenant_email, property_name(&properties, lease), room_name(&properties, lease)).to_lowercase().contains(&term))
    }).cloned().collect();
    let selected = visible.iter().find(|lease| lease.id == selected_id()).or_else(|| visible.first()).cloned();
    let displayed_id = selected.as_ref().map(|lease| lease.id).unwrap_or(0);

    rsx! {
        div { class: "occupancy-page",
            div { class: "occupancy-heading",
                div { h2 { "Signed tenancies" } p { "The signed lease determines the guest, property, rental unit and stay. Confirm each signature to activate the tenant account." } }
                div { class: "occupancy-totals", span { strong { "{signed}" } " To confirm" } span { strong { "{confirmed}" } " Confirmed" } }
            }
            if !message().is_empty() { p { class: "lease-notice", role: "status", "{message}" } }
            div { class: "occupancy-layout",
                crate::searchable_list::SearchableList { class: "occupancy-list",
                    search_placeholder: "Search tenant, property or rental unit", search_value: search(), on_search: move |value| search.set(value),
                    div { class: "searchable-list-filters occupancy-filters", role: "group", aria_label: "Filter tenancies",
                        for (key, label) in [("all", "All"), ("signed", "To confirm"), ("confirmed", "Confirmed")] {
                            button { key: "{key}", r#type: "button", class: if filter() == key { "active" } else { "" }, onclick: move |_| filter.set(key.into()), "{label}" }
                        }
                    }
                    if loading() { p { class: "occupancy-empty", "Loading signed leases…" } }
                    else if all.is_empty() { div { class: "occupancy-empty", h3 { "No signed leases yet" } p { "When a guest signs a lease, their rental unit will appear here automatically." } } }
                    else if visible.is_empty() { p { class: "occupancy-empty", "No tenancies match this view." } }
                    else { div { class: "searchable-list-items occupancy-rows",
                        for lease in visible {
                            button { key: "{lease.id}", r#type: "button", class: if displayed_id == lease.id { "occupancy-row register-row selected" } else { "occupancy-row register-row" }, aria_pressed: displayed_id == lease.id, onclick: { let id = lease.id; move |_| selected_id.set(id) },
                                span { class: "occupancy-avatar", "{lease.tenant_name.chars().next().unwrap_or('?').to_uppercase()}" }
                                span { class: "occupancy-row-text", strong { "{lease.tenant_name}" } small { "{property_name(&properties, &lease)} · {room_name(&properties, &lease)}" } small { "{date_label(&lease.details.start_date)} – {date_label(&lease.details.end_date)}" } }
                                span { class: if lease.status == "signed" { "occupancy-status pending" } else { "occupancy-status" }, if lease.status == "signed" && lease.landlord_signed_at.is_none() { "Landlord to sign" } else if lease.status == "signed" { "To confirm" } else { "Confirmed" } }
                            }
                        }
                    } }
                }
                section { class: "occupancy-detail", aria_label: "Tenancy details",
                    if let Some(lease) = selected {
                        div { class: "occupancy-detail-head", span { "LEASE #{lease.id}" } span { class: if lease.status == "signed" { "occupancy-status pending" } else { "occupancy-status" }, if lease.status == "signed" && lease.landlord_signed_at.is_none() { "Awaiting landlord" } else if lease.status == "signed" { "Both signed" } else { "Confirmed" } } }
                        h2 { "{lease.tenant_name}" }
                        p { class: "occupancy-email", "{lease.tenant_email}" }
                        div { class: "occupancy-facts",
                            div { span { "PROPERTY" } strong { "{property_name(&properties, &lease)}" } }
                            div { span { "RENTAL UNIT" } strong { "{room_name(&properties, &lease)}" } }
                            div { span { "MOVE-IN" } strong { "{date_label(&lease.details.start_date)}" } }
                            div { span { "STAY THROUGH" } strong { "{date_label(&lease.details.end_date)}" } }
                            div { span { "MONTHLY RENT" } strong { "C${lease.details.monthly_rent}" } }
                            div { span { "SIGNED" } strong { if lease.signed_at.is_some() { "Received" } else { "Recorded" } } }
                        }
                        if lease.status == "signed" { p { class: "occupancy-next", if lease.landlord_signed_at.is_none() { "The tenant signed. Waiting for the landlord to sign the completed lease." } else if lease.photo_id_verified { "Review the signed lease, then confirm the tenancy to activate the tenant account." } else if lease.photo_id_uploaded { "Review and approve the uploaded photo ID in the lease dashboard before confirmation." } else { "Waiting for the tenant to upload a photo ID before confirmation." } } }
                        else if lease.copy_sent_at.is_none() { p { class: "occupancy-next", "The tenant account is active. The signed copy still needs to be delivered." } }
                        else { p { class: "occupancy-next", "The tenant account is active and the signed copy was sent." } }
                        div { class: "occupancy-actions",
                            a { class: "outline-button", href: "/api/leases/{lease.id}/document", target: "_blank", span { "Review signed lease" } Icon { name: "arrow-up-right" } }
                            if lease.photo_id_uploaded { a { class: "outline-button", href: "/api/leases/{lease.id}/photo-id/front", target: "_blank", span { "Review photo ID" } Icon { name: "arrow-up-right" } } }
                            if lease.status == "signed" { button { class: "primary-button", r#type: "button", disabled: busy() || !lease.photo_id_verified || lease.landlord_signed_at.is_none(), onclick: { let id = lease.id; move |_| {
                                busy.set(true);
                                spawn(async move {
                                    let result = Request::post(&format!("/api/leases/{id}/confirm")).send().await;
                                    let mut confirmed = false;
                                    if let Ok(response) = Request::get("/api/leases").send().await {
                                        if let Ok(items) = response.json::<Vec<OccupancyLease>>().await {
                                            confirmed = items.iter().any(|lease| lease.id == id && lease.status == "confirmed");
                                            leases.set(items);
                                        }
                                    }
                                    match result {
                                        Ok(response) if response.ok() => message.set("Tenancy confirmed.".into()),
                                        Ok(response) if confirmed => { let _ = response.text().await; message.set("Tenancy confirmed. Signed copy delivery needs attention; open the lease register to resend it.".into()) },
                                        Ok(response) => message.set(response.text().await.unwrap_or_else(|_| "Could not confirm tenancy.".into())),
                                        Err(_) if confirmed => message.set("Tenancy confirmed. Check signed copy delivery in the lease register.".into()),
                                        Err(_) => message.set("Could not confirm tenancy.".into()),
                                    }
                                    busy.set(false);
                                });
                            } }, "Confirm tenancy" } }
                            if lease.status == "confirmed" {
                                if let Some(account) = accounts().iter().find(|account| account.role == "tenant" && account.email.eq_ignore_ascii_case(&lease.tenant_email)) {
                                    a { class: "outline-button", href: "/access/view?user={account.id}", span { "Manage tenant access" } Icon { name: "arrow-up-right" } }
                                }
                                if lease.copy_sent_at.is_none() { a { class: "outline-button", href: "/leases/view?lease={lease.id}", span { "Manage delivery" } Icon { name: "arrow-right" } } }
                            }
                        }
                    } else {
                        div { class: "occupancy-empty detail", h3 { "Select a tenancy" } p { "Choose a signed lease to see the assigned rental unit and next step." } }
                    }
                }
            }
        }
    }
}
