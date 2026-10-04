mod api;
mod availability_page;
mod guest_model;
mod guest_pages;
mod map;
mod model;
mod photo_manager;
mod pricing_page;
mod property_page;
mod tour_api;
mod tour_model;
mod tour_pages;

use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use model::{Property, Space};

static CSS: Asset = asset!("/assets/main.css");
static PORTFOLIO_MAP: Asset = asset!("/assets/portfolio-map.html");
const STORAGE_KEY: &str = "buildry.properties.v2";

fn starter_properties() -> Vec<Property> {
    let addresses = [
        "315 Dearborn Blvd",
        "200 Holbeach Crescent",
        "94 Blythwood Road, Waterloo, ON",
        "112 Blythwood Rd",
        "412 Spice Bush St",
        "236 Inverhuron Crescent",
    ];
    addresses
        .iter()
        .enumerate()
        .map(|(index, address)| {
            let location = starter_location(address);
            Property {
                id: (index + 1) as u32,
                name: (*address).to_string(),
                area: "Waterloo, ON".into(),
                address: (*address).to_string(),
                kind: "Not set".into(),
                ensuite_rooms: 0,
                shared_rooms: 0,
                suites: 0,
                notes:
                    "Address supplied. Match this house to its room layout before adding inventory."
                        .into(),
                amenities: Vec::new(),
                photos: Vec::new(),
                latitude: location.map(|(latitude, _, _)| latitude),
                longitude: location.map(|(_, longitude, _)| longitude),
                map_label: location.map(|(_, _, label)| label.into()),
                spaces: Vec::new(),
            }
        })
        .collect()
}

// These house-level matches were checked against OpenStreetMap's Nominatim
// data. Keep them keyed by address so existing browser catalogs can gain the
// map without overwriting any manually refreshed or edited pin.
fn starter_location(address: &str) -> Option<(f64, f64, &'static str)> {
    match address {
        "315 Dearborn Blvd" => Some((
            43.4909610,
            -80.5178159,
            "315 Dearborn Boulevard, Waterloo, ON",
        )),
        "200 Holbeach Crescent" => Some((
            43.4881880,
            -80.5199700,
            "200 Holbeach Crescent, Waterloo, ON",
        )),
        "94 Blythwood Road, Waterloo, ON" => {
            Some((43.4869047, -80.5352416, "94 Blythwood Road, Waterloo, ON"))
        }
        "112 Blythwood Rd" => Some((43.4860465, -80.5371826, "112 Blythwood Road, Waterloo, ON")),
        "412 Spice Bush St" => Some((
            43.4677112,
            -80.5976251,
            "412 Spice Bush Street, Waterloo, ON",
        )),
        "236 Inverhuron Crescent" => Some((
            43.4950626,
            -80.5727879,
            "236 Inverhuron Crescent, Waterloo, ON",
        )),
        _ => None,
    }
}

fn load_properties() -> Vec<Property> {
    let mut properties: Vec<Property> =
        LocalStorage::get(STORAGE_KEY).unwrap_or_else(|_| starter_properties());
    let mut changed = false;
    for property in &mut properties {
        if property.spaces.is_empty()
            && (property.ensuite_rooms + property.shared_rooms + property.suites > 0)
        {
            property.migrate_legacy_counts();
            changed = true;
        }
        if property.latitude.is_none() && property.longitude.is_none() {
            if let Some((latitude, longitude, label)) = starter_location(&property.address) {
                property.latitude = Some(latitude);
                property.longitude = Some(longitude);
                property.map_label = Some(label.into());
                changed = true;
            }
        }
    }
    if changed {
        let _ = LocalStorage::set(STORAGE_KEY, &properties);
    }
    properties
}

fn update_draft(mut draft: Signal<Option<Property>>, change: impl FnOnce(&mut Property)) {
    let mut current = draft.write();
    if let Some(property) = current.as_mut() {
        change(property);
    }
}

fn update_space_draft(mut draft: Signal<Option<(u32, Space)>>, change: impl FnOnce(&mut Space)) {
    let mut current = draft.write();
    if let Some((_, space)) = current.as_mut() {
        change(space);
    }
}

fn main() {
    let path = web_sys::window()
        .and_then(|window| window.location().pathname().ok())
        .unwrap_or_default();
    if path.starts_with("/booking/") {
        dioxus::launch(tour_pages::guest_manage_page);
        return;
    }
    if path.starts_with("/stays/house/") || path.starts_with("/stays/room/") {
        dioxus::launch(guest_pages::guest_detail_page);
        return;
    }
    if path.starts_with("/properties/") {
        dioxus::launch(property_page::property_page);
        return;
    }
    match path.as_str() {
        "/pricing" => dioxus::launch(pricing_page::pricing_page),
        "/stays" => dioxus::launch(guest_pages::guest_stays_page),
        "/book" => dioxus::launch(tour_pages::guest_page),
        "/tours" => dioxus::launch(tour_pages::dashboard_page),
        "/availability" => dioxus::launch(availability_page::availability_page),
        _ => dioxus::launch(app),
    }
}

fn app() -> Element {
    let mut properties = use_signal(Vec::<Property>::new);
    let mut connection_status = use_signal(|| "Connecting to PostgreSQL…".to_string());
    let mut ready = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut selected_id = use_signal(|| 1_u32);
    let mut draft = use_signal(|| None::<Property>);
    let mut draft_space = use_signal(|| None::<(u32, Space)>);
    let mut search = use_signal(String::new);
    let mut locating_id = use_signal(|| None::<u32>);
    let mut map_status = use_signal(|| None::<(u32, String)>);
    let mut sidebar_collapsed =
        use_signal(|| LocalStorage::get::<bool>("buildry.sidebar.collapsed").unwrap_or(false));
    use_effect(move || {
        spawn(async move {
            match api::bootstrap(&load_properties()).await {
                Ok(saved) => {
                    properties.set(saved);
                    ready.set(true);
                    connection_status.set("Saved in PostgreSQL".into());
                }
                Err(error) => connection_status.set(error),
            }
        });
    });
    let list = properties();
    let selected = list
        .iter()
        .find(|p| p.id == selected_id())
        .or_else(|| list.first())
        .cloned();
    let query = search().to_lowercase();
    let visible: Vec<Property> = list
        .iter()
        .filter(|p| {
            p.name.to_lowercase().contains(&query)
                || p.area.to_lowercase().contains(&query)
                || p.address.to_lowercase().contains(&query)
                || p.id.to_string().contains(&query)
        })
        .cloned()
        .collect();
    let visible_is_empty = visible.is_empty();
    let total_bedrooms: usize = list.iter().map(|p| p.kind_count("Bedroom")).sum();
    let total_suites: usize = list.iter().map(|p| p.kind_count("Suite")).sum();
    let pins: Vec<_> = list
        .iter()
        .filter_map(|property| {
            Some(serde_json::json!({
                "name": format!("House {} · {}", property.id, property.name),
                "address": property.address,
                "latitude": property.latitude?,
                "longitude": property.longitude?,
            }))
        })
        .collect();
    let portfolio_map_url = format!(
        "{}?pins={}",
        PORTFOLIO_MAP,
        urlencoding::encode(&serde_json::to_string(&pins).unwrap_or_default())
    );

    rsx! {
        document::Stylesheet { href: CSS }
        div { class: "app-shell",
            aside { class: if sidebar_collapsed() { "sidebar collapsed" } else { "sidebar" },
                div { class: "sidebar-header",
                    div { class: "brand", span { class: "brand-mark", "B" } span { class: "brand-name", "buildry" } }
                    button { class: "sidebar-toggle", r#type: "button", aria_label: if sidebar_collapsed() { "Expand sidebar" } else { "Collapse sidebar" }, onclick: move |_| { let next = !sidebar_collapsed(); sidebar_collapsed.set(next); let _ = LocalStorage::set("buildry.sidebar.collapsed", next); }, if sidebar_collapsed() { "›" } else { "‹" } }
                }
                a { class: "nav-item active", href: "/", title: "Properties", span { class: "nav-icon icon-properties" } span { class: "nav-text", "Properties" } span { class: "nav-count", "{list.len()}" } }
                a { class: "nav-item tour-nav-link", href: "/pricing", title: "Pricing", span { class: "nav-icon icon-pricing" } span { class: "nav-text", "Pricing" } }
                a { class: "nav-item tour-nav-link", href: "/tours", title: "House tours", span { class: "nav-icon icon-tours" } span { class: "nav-text", "House tours" } }
                a { class: "nav-item tour-nav-link", href: "/availability", title: "Availability", span { class: "nav-icon icon-availability" } span { class: "nav-text", "Availability" } }
                a { class: "nav-item tour-nav-link", href: "/stays", title: "Guest stays", span { class: "nav-icon icon-stays" } span { class: "nav-text", "Guest stays" } }
                form { class: "nav-logout", method: "post", action: "/logout", button { class: "nav-item tour-nav-link", r#type: "submit", title: "Sign out", span { class: "nav-icon icon-logout" } span { class: "nav-text", "Sign out" } } }
            }
            main { class: "main-content catalog-main",
                section { class: "page-head",
                    div {
                        h1 { "Properties" }
                        p { class: "catalog-summary", "{list.len()} houses · {total_bedrooms} bedrooms · {total_suites} suites" }
                    }
                    div { class: "page-actions",
                        a { class: "primary-button", href: "/properties/new", "+ Add property" }
                    }
                }
                if connection_status() != "Saved in PostgreSQL" { p { class: "storage-status", "{connection_status}" } }
                section { class: "portfolio-map-panel",
                    h2 { "Locations" }
                    if pins.is_empty() {
                        div { class: "map-placeholder portfolio-map-empty", p { "No locations yet" } }
                    } else {
                        iframe { class: "portfolio-map-frame", title: "Map of all property locations", src: portfolio_map_url }
                    }
                }
                div { class: "content-grid",
                    section { class: "catalog-panel",
                        div { class: "panel-heading",
                            div { h2 { "Houses" } }
                            if !search().is_empty() { span { class: "result-count", "{visible.len()} found" } }
                        }
                        input { class: "search-input", placeholder: "Search properties", value: "{search}", oninput: move |e| search.set(e.value()) }
                        div { class: "property-list",
                            for property in visible {
                                a { key: "{property.id}", href: "/properties/{property.id}", class: "property-row",
                                    span { class: "house-number", "{property.id}" }
                                    span { class: "property-row-copy", strong { "{property.name}" } small { if property.address.trim().is_empty() { "Address not set" } else { "{property.address}" } } }
                                    span { class: "space-pill", "{property.inventory_label()}" }
                                }
                            }
                            if list.is_empty() { p { class: "empty", "No properties yet. Add your first house." } }
                            if !list.is_empty() && visible_is_empty { p { class: "empty", "No properties match that search." } }
                        }
                    }
                    section { class: "detail-panel",
                        if let Some(property) = selected {
                            div { class: "detail-body",
                                div { class: "detail-title-row",
                                    div { h2 { "{property.name}" } }
                                    button { class: "outline-button", onclick: { let editing = property.clone(); move |_| draft.set(Some(editing.clone())) }, "Edit" }
                                }
                                if property.address != property.name && !property.address.is_empty() { p { class: "detail-address", "{property.address}" } }
                                div { class: "divider" }
                                div { class: "space-heading",
                                    div { h3 { "Rooms & areas" } }
                                    button { class: "outline-button", onclick: { let id = property.id; let next = property.spaces.iter().map(|space| space.id).max().unwrap_or(0) + 1; move |_| draft_space.set(Some((id, Space::new(next)))) }, "+ Add space" }
                                }
                                if property.spaces.is_empty() {
                                    p { class: "inventory-pending", "No spaces recorded yet. Add areas such as Basement, then add their bedrooms and bathrooms." }
                                } else {
                                    div { class: "physical-space-list",
                                        for space in property.spaces.iter() {
                                            button { class: if space.parent_id.is_some() { "physical-space-row nested" } else { "physical-space-row" }, key: "{space.id}", onclick: { let id = property.id; let editing = space.clone(); move |_| draft_space.set(Some((id, editing.clone()))) },
                                                span { class: "space-row-copy", strong { "{space.name}" }
                                                    if space.bathroom_access == "Separate" { small { "Separate bath" } }
                                                    else if space.bathroom_access == "Ensuite" && !space.name.to_lowercase().contains("ensuite") { small { "Ensuite" } }
                                                }
                                                if let Some(rent) = space.current_monthly_rent { span { class: "rent-pill", "${rent}/mo" } }
                                                if space.rent_is_estimate { span { class: "rent-estimate", "Example" } }
                                                span { class: "room-chevron", "›" }
                                            }
                                        }
                                    }
                                }
                                if !property.notes.is_empty() && !property.notes.starts_with("Room breakdown entered from the supplied report.") && !property.notes.starts_with("Address supplied. Match this house") {
                                    h3 { class: "notes-heading", "Notes" }
                                    p { class: "notes", "{property.notes}" }
                                }
                                if property.latitude.is_none() || property.longitude.is_none() { div { class: "map-section",
                                    div { class: "map-heading", h3 { "Location" } }
                                    if let Some((status_id, message)) = map_status() {
                                        if status_id == property.id { p { class: "map-status", "{message}" } }
                                    }
                                    button { class: "outline-button map-button", disabled: locating_id().is_some(), onclick: {
                                        let id = property.id;
                                        let address = property.address.clone();
                                        move |_| {
                                            if locating_id().is_some() { return; }
                                            let address = address.clone();
                                            locating_id.set(Some(id));
                                            map_status.set(None);
                                            spawn(async move {
                                                match map::locate(&address).await {
                                                    Ok(found) => {
                                                        let mut updated = properties();
                                                        if let Some(saved) = updated.iter_mut().find(|item| item.id == id && item.address == address) {
                                                            saved.latitude = Some(found.latitude);
                                                            saved.longitude = Some(found.longitude);
                                                            saved.map_label = Some(found.display_name);
                                                            match api::save(saved).await {
                                                                Ok(_) => {
                                                                    properties.set(updated);
                                                                    map_status.set(Some((id, "Pin saved in PostgreSQL. Check it against the actual location.".into())));
                                                                }
                                                                Err(error) => map_status.set(Some((id, error))),
                                                            }
                                                        }
                                                    }
                                                    Err(message) => map_status.set(Some((id, message))),
                                                }
                                                locating_id.set(None);
                                            });
                                        }
                                    }, if locating_id() == Some(property.id) { "Locating…" } else { "Locate address" } }
                                } }
                            }
                        } else {
                            p { class: "empty", "Select or add a property to view its profile." }
                        }
                    }
                }
            }
        }
        if let Some(item) = draft() {
            div { class: "modal-backdrop",
                div { class: "edit-panel",
                    div { class: "edit-header", div { h2 { if list.iter().any(|p| p.id == item.id) { "Edit property" } else { "Add property" } } }, button { class: "close-button", onclick: move |_| draft.set(None), "×" } }
                    form { onsubmit: move |event| {
                        event.prevent_default();
                        if saving() { return; }
                        if let Some(mut saved) = draft() {
                            saved.name = saved.name.trim().to_string();
                            if saved.name.is_empty() { return; }
                            if let Some(existing) = properties.read().iter().find(|p| p.id == saved.id) {
                                if existing.address != saved.address { saved.latitude = None; saved.longitude = None; saved.map_label = None; }
                            }
                            saving.set(true);
                            connection_status.set("Saving property…".into());
                            spawn(async move {
                                match api::save(&saved).await {
                                    Ok(saved) => {
                                        let mut updated = properties();
                                        if let Some(existing) = updated.iter_mut().find(|p| p.id == saved.id) { *existing = saved.clone(); } else { updated.push(saved.clone()); }
                                        properties.set(updated);
                                        selected_id.set(saved.id);
                                        draft.set(None);
                                        connection_status.set("Saved in PostgreSQL".into());
                                    }
                                    Err(error) => connection_status.set(error),
                                }
                                saving.set(false);
                            });
                        }
                    },
                        label { "Property name", input { required: true, value: item.name.clone(), placeholder: "e.g. Hazel House A", oninput: move |e| update_draft(draft, |p| p.name = e.value()) } }
                        label { "Area", input { value: item.area.clone(), placeholder: "e.g. Hazel & Columbia", oninput: move |e| update_draft(draft, |p| p.area = e.value()) } }
                        label { "Street address", input { value: item.address.clone(), placeholder: "Add when ready", oninput: move |e| update_draft(draft, |p| p.address = e.value()) } }
                        label { "Notes", textarea { value: item.notes.clone(), rows: "4", placeholder: "Layout, access, and details to confirm", oninput: move |e| update_draft(draft, |p| p.notes = e.value()) } }
                        label { "Confirmed amenities (comma separated)", textarea { value: item.amenities.join(", "), rows: "2", placeholder: "e.g. Wi-Fi, laundry, parking", oninput: move |e| update_draft(draft, |p| p.amenities = e.value().split(',').map(str::trim).filter(|value| !value.is_empty()).map(str::to_string).collect()) } }
                        div { class: "form-actions", button { r#type: "button", class: "text-button", onclick: move |_| draft.set(None), "Cancel" } button { r#type: "submit", class: "primary-button", disabled: saving(), if saving() { "Saving…" } else { "Save property" } } }
                    }
                }
            }
        }
        if let Some((property_id, space)) = draft_space() {
            div { class: "modal-backdrop",
                div { class: "edit-panel",
                    div { class: "edit-header",
                        div { h2 { if list.iter().any(|property| property.id == property_id && property.spaces.iter().any(|item| item.id == space.id)) { "Edit space" } else { "Add space" } } }
                        button { class: "close-button", onclick: move |_| draft_space.set(None), "×" }
                    }
                    form { onsubmit: move |event| {
                        event.prevent_default();
                        if saving() { return; }
                        if let Some((property_id, mut space)) = draft_space() {
                            space.name = space.name.trim().to_string();
                            if space.name.is_empty() { return; }
                            if space.kind == "Area" || space.kind == "Suite" { space.parent_id = None; }
                            if space.kind != "Bedroom" { space.bathroom_access.clear(); }
                            let Some(mut property) = properties.read().iter().find(|item| item.id == property_id).cloned() else { return; };
                            if let Some(existing) = property.spaces.iter_mut().find(|item| item.id == space.id) { *existing = space; } else { property.spaces.push(space); }
                            saving.set(true);
                            connection_status.set("Saving space…".into());
                            spawn(async move {
                                match api::save(&property).await {
                                    Ok(saved) => {
                                        let mut updated = properties();
                                        if let Some(existing) = updated.iter_mut().find(|item| item.id == saved.id) { *existing = saved; }
                                        properties.set(updated);
                                        draft_space.set(None);
                                        connection_status.set("Saved in PostgreSQL".into());
                                    }
                                    Err(error) => connection_status.set(error),
                                }
                                saving.set(false);
                            });
                        }
                    },
                        label { "Space name", input { required: true, value: space.name.clone(), placeholder: "e.g. Bedroom 1 or Basement", oninput: move |e| update_space_draft(draft_space, |item| item.name = e.value()) } }
                        label { "Type", select { value: space.kind.clone(), oninput: move |e| update_space_draft(draft_space, |item| item.kind = e.value()),
                            option { value: "Bedroom", "Bedroom" }
                            option { value: "Bathroom", "Bathroom" }
                            option { value: "Area", "Area / group" }
                            option { value: "Suite", "Suite / group" }
                            option { value: "Kitchen", "Kitchen" }
                            option { value: "Living space", "Living space" }
                            option { value: "Other", "Other" }
                        } }
                        if space.kind != "Area" && space.kind != "Suite" {
                            label { "Inside area", select { value: space.parent_id.map(|id| id.to_string()).unwrap_or_default(), oninput: move |e| update_space_draft(draft_space, |item| item.parent_id = e.value().parse().ok()),
                                option { value: "", "Whole property / no area" }
                                if let Some(property) = list.iter().find(|item| item.id == property_id) {
                                    for area in property.spaces.iter().filter(|item| (item.kind == "Area" || item.kind == "Suite") && item.parent_id.is_none() && item.id != space.id) {
                                        option { key: "{area.id}", value: "{area.id}", "{area.name}" }
                                    }
                                }
                            } }
                        }
                        if space.kind == "Bedroom" {
                            label { "Bathroom access", select { value: space.bathroom_access.clone(), oninput: move |e| update_space_draft(draft_space, |item| item.bathroom_access = e.value()),
                                option { value: "", "Not recorded" }
                                option { value: "Ensuite", "Ensuite" }
                                option { value: "Separate", "Separate bathroom" }
                            } }
                        }
                        label { "Current monthly rent (reference)", input { r#type: "number", min: "0", value: space.current_monthly_rent.map(|rent| rent.to_string()).unwrap_or_default(), placeholder: "e.g. 950", oninput: move |e| update_space_draft(draft_space, |item| { item.current_monthly_rent = e.value().parse().ok(); item.rent_is_estimate = false; }) } }
                        if space.rent_is_estimate { p { class: "rent-estimate-note", "Example rent. Enter the confirmed amount to replace it." } }
                        if space.kind == "Bedroom" || space.kind == "Suite" {
                            p { class: "rent-estimate-note", "Guest quotes use the seasonal rates below. September must be higher than January, and January higher than May." }
                            div { class: "seasonal-rent-inputs",
                                label { "Sep–Dec / month", input { r#type: "number", min: "1", value: space.rent_sep_dec.map(|rent| rent.to_string()).unwrap_or_default(), placeholder: "e.g. 1050", oninput: move |e| update_space_draft(draft_space, |item| { item.rent_sep_dec = e.value().parse().ok(); item.seasonal_prices_confirmed = false; }) } }
                                label { "Jan–Apr / month", input { r#type: "number", min: "1", value: space.rent_jan_apr.map(|rent| rent.to_string()).unwrap_or_default(), placeholder: "e.g. 950", oninput: move |e| update_space_draft(draft_space, |item| { item.rent_jan_apr = e.value().parse().ok(); item.seasonal_prices_confirmed = false; }) } }
                                label { "May–Aug / month", input { r#type: "number", min: "1", value: space.rent_may_aug.map(|rent| rent.to_string()).unwrap_or_default(), placeholder: "e.g. 850", oninput: move |e| update_space_draft(draft_space, |item| { item.rent_may_aug = e.value().parse().ok(); item.seasonal_prices_confirmed = false; }) } }
                            }
                            label { class: "seasonal-confirm-label", input { r#type: "checkbox", checked: space.seasonal_prices_confirmed, onchange: move |e| update_space_draft(draft_space, |item| item.seasonal_prices_confirmed = e.checked()) } "These seasonal rates are confirmed" }
                        }
                        label { "Level (optional)", input { value: space.level.clone(), placeholder: "e.g. Main floor, lower level", oninput: move |e| update_space_draft(draft_space, |item| item.level = e.value()) } }
                        label { "Notes", textarea { value: space.notes.clone(), rows: "3", placeholder: "Physical details to remember", oninput: move |e| update_space_draft(draft_space, |item| item.notes = e.value()) } }
                        if space.kind == "Bedroom" || space.kind == "Suite" {
                            label { "Confirmed room amenities (comma separated)", textarea { value: space.amenities.join(", "), rows: "2", placeholder: "e.g. Furnished, desk, closet", oninput: move |e| update_space_draft(draft_space, |item| item.amenities = e.value().split(',').map(str::trim).filter(|value| !value.is_empty()).map(str::to_string).collect()) } }
                        }
                        if connection_status() != "Saved in PostgreSQL" { p { class: "space-form-status", "{connection_status}" } }
                        div { class: "form-actions", button { r#type: "button", class: "text-button", onclick: move |_| draft_space.set(None), "Cancel" } button { r#type: "submit", class: "primary-button", disabled: saving(), if saving() { "Saving…" } else { "Save space" } } }
                    }
                }
            }
        }
    }
}
