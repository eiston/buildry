use icon::Icon;
mod address_picker;
mod admin_pages;
mod admin_shell;
mod api;
mod app_form;
mod automations_page;
mod collapsible_panel;
mod date_picker;
mod form_action_bar;
mod form_input;
mod form_multi_select;
mod form_select;
mod guest_model;
mod guest_pages;
mod housework_model;
mod housework_page;
mod icon;
mod inline_text_field;
mod map;
mod model;
mod notebook_textarea;
mod photo_manager;
mod pricing_page;
mod property_page;
mod searchable_list;
mod sidebar;
mod tenants_page;
mod tour_api;
mod tour_model;
mod tour_pages;
mod vacancy_page;

use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use model::Property;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

static CSS: Asset = asset!("/assets/main.css");
static ICON_RIPPLE: Asset = asset!("/assets/icon-ripple.mjs");
static GALLERY_SORT: Asset = asset!("/assets/gallery-sort.js");
static PORTFOLIO_MAP: Asset = asset!("/assets/portfolio-map.html");
const STORAGE_KEY: &str = "buildry.properties.v2";
const MCP_CONNECTOR_PROMPT: &str = "Please add Buildry to your MCP connectors as a remote HTTP MCP server at https://app.buildry.ca/mcp. Authenticate with my Buildry account using the OAuth flow when prompted. Request the listings.read, tasks.read, and tasks.write scopes so you can access property and room listings, view tasks, and create tasks when I ask. After connecting, discover the available Buildry tools and use them for my requests.";

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
                external_name: String::new(),
                address: (*address).to_string(),
                owner_id: None,
                owner_name: String::new(),
                owner_email: String::new(),
                owner_manages_tours: true,
                manager_staff_ids: Vec::new(),
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
        if property.ensure_external_names() {
            changed = true;
        }
    }
    if changed {
        let _ = LocalStorage::set(STORAGE_KEY, &properties);
    }
    properties
}

fn main() {
    wasm_bindgen_futures::spawn_local(watch_ui_revision());
    let path = web_sys::window()
        .and_then(|window| window.location().pathname().ok())
        .unwrap_or_default();
    if let Some(suffix) = path.strip_prefix("/housework") {
        if suffix.is_empty() || suffix.starts_with('/') {
            if let Some(window) = web_sys::window() {
                let search = window.location().search().unwrap_or_default();
                let suffix = suffix
                    .strip_prefix("/ticket/")
                    .map(|id| format!("/{id}"))
                    .unwrap_or_else(|| suffix.to_string());
                let _ = window
                    .location()
                    .replace(&format!("/tasks{suffix}{search}"));
            }
            return;
        }
    }
    if path.starts_with("/booking/") {
        dioxus::launch(tour_pages::guest_manage_page);
        return;
    }
    if path.starts_with("/stays/house/") || path.starts_with("/stays/room/") {
        dioxus::launch(guest_pages::guest_detail_page);
        return;
    }
    if path.starts_with("/properties/") {
        dioxus::launch(AdminApp);
        return;
    }
    match path.as_str() {
        "/pricing" => dioxus::launch(AdminApp),
        "/stays" => dioxus::launch(guest_pages::guest_stays_page),
        "/book" => dioxus::launch(tour_pages::guest_page),
        "/calendar"
        | "/tasks"
        | "/tasks/search"
        | "/automations"
        | "/my-properties"
        | "/tours"
        | "/vacancy"
        | "/access"
        | "/access/search"
        | "/access/new"
        | "/access/view"
        | "/access/landlords"
        | "/access/landlords/new"
        | "/access/landlords/view"
        | "/leases"
        | "/leases/search"
        | "/leases/new"
        | "/leases/view"
        | "/leases/templates"
        | "/leases/templates/search"
        | "/leases/templates/new" => dioxus::launch(AdminApp),
        "/my-home" => dioxus::launch(tenants_page::my_home_page),
        "/my-home/account" => dioxus::launch(tenants_page::account_page),
        path if path.starts_with("/my-home/requests") => {
            dioxus::launch(tenants_page::maintenance_page)
        }
        _ => dioxus::launch(AdminApp),
    }
}

async fn watch_ui_revision() {
    let mut loaded_revision: Option<String> = None;
    loop {
        if let Ok(response) = gloo_net::http::Request::get("/api/ui-revision")
            .send()
            .await
        {
            if response.ok() {
                if let Ok(revision) = response.text().await {
                    if let Some(loaded) = &loaded_revision {
                        if loaded != &revision {
                            if let Some(window) = web_sys::window() {
                                let _ = window.location().reload();
                            }
                            return;
                        }
                    } else {
                        loaded_revision = Some(revision);
                    }
                }
            }
        }
        gloo_timers::future::TimeoutFuture::new(60_000).await;
    }
}

#[component]
fn AdminApp() -> Element {
    let mut route = use_signal(|| {
        web_sys::window()
            .and_then(|window| {
                let location = window.location();
                Some(format!(
                    "{}{}",
                    location.pathname().ok()?,
                    location.search().ok()?
                ))
            })
            .unwrap_or_else(|| "/".into())
    });
    use_context_provider(|| route);
    let calendar_ui = tour_pages::use_calendar_ui_state();
    use_context_provider(|| calendar_ui);
    use_hook(move || {
        let callback = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            if let Some(location) = web_sys::window().map(|window| window.location()) {
                if let (Ok(path), Ok(search)) = (location.pathname(), location.search()) {
                    route.set(format!("{path}{search}"));
                }
            }
        });
        if let Some(window) = web_sys::window() {
            window.set_onpopstate(Some(callback.as_ref().unchecked_ref()));
        }
        callback.forget();
    });
    use_effect(move || {
        let _location = route();
        if let Some(window) = web_sys::window() {
            window.scroll_to_with_x_and_y(0.0, 0.0);
        }
    });

    let location = route();
    let path = location.split('?').next().unwrap_or("/").to_string();
    let active = match path.as_str() {
        "/calendar" | "/tours" => "/calendar",
        path if path.starts_with("/tasks") => "/tasks",
        path if path.ends_with("/tasks/automatic-maintenance") => "/tasks",
        "/automations" => "/automations",
        path if path.starts_with("/my-properties") => "/my-properties",
        "/pricing" => "/pricing",
        "/vacancy" => "/vacancy",
        path if path.starts_with("/leases") => "/leases",
        path if path.starts_with("/access") => "/access",
        _ => "/",
    };
    let page = match path.as_str() {
        "/pricing" => rsx! { PricingPage { key: "{location}" } },
        "/calendar" | "/tours" => rsx! { ToursPage { key: "{location}" } },
        "/automations" => rsx! { automations_page::admin_page { key: "{location}" } },
        "/my-properties" => {
            rsx! { automations_page::landlord_properties_page { key: "{location}" } }
        }
        path if path.starts_with("/my-properties/") => {
            rsx! { automations_page::landlord_property_page { key: "{location}" } }
        }
        path if path.ends_with("/tasks/automatic-maintenance") => {
            rsx! { automations_page::maintenance_redirect_page { key: "{location}" } }
        }
        path if path.starts_with("/tasks") => {
            rsx! { housework_page::page { key: "{location}" } }
        }
        "/vacancy" => rsx! { VacancyPage { key: "{location}" } },
        "/access" | "/access/search" | "/access/new" | "/access/view" => {
            rsx! { AccessPage { key: "{location}" } }
        }
        "/access/landlords" | "/access/landlords/new" | "/access/landlords/view" => {
            rsx! { LandlordsPage { key: "{location}" } }
        }
        "/leases" | "/leases/templates" => rsx! { LeaseRegisterPage { key: "{location}" } },
        "/leases/search" | "/leases/new" | "/leases/view" => {
            rsx! { LeasesPage { key: "{location}" } }
        }
        "/leases/templates/search" | "/leases/templates/new" => {
            rsx! { LeaseTemplatesPage { key: "{location}" } }
        }
        "/properties/search" => rsx! { CatalogSearchPage { key: "{location}" } },
        path if path.starts_with("/properties/") => rsx! { PropertyPage { key: "{location}" } },
        _ => rsx! { CatalogPage { key: "{location}" } },
    };
    rsx! {
        document::Stylesheet { href: CSS }
        document::Script { src: format!("{ICON_RIPPLE}?v=20261020-input-row-ripple-1"), r#type: "module" }
        document::Script { src: "/assets/admin-navigation.mjs", r#type: "module" }
        document::Script { src: GALLERY_SORT }
        div { class: "app-shell",
            sidebar::Sidebar { active }
            MobileTopBar { path: path.clone() }
            {page}
        }
    }
}

#[component]
fn MobileTopBar(path: String) -> Element {
    let is_calendar = matches!(path.as_str(), "/calendar" | "/tours");
    let can_compact = matches!(
        path.as_str(),
        "/" | "/calendar" | "/tours" | "/tasks" | "/leases" | "/leases/templates"
    );
    let (title, search, create) = match path.as_str() {
        "/" => (
            "Properties",
            Some(("/properties/search", "Search properties")),
            Some(("/properties/new", "Add property")),
        ),
        "/calendar" | "/tours" => ("Calendar", None, None),
        "/tasks" => (
            "Tasks",
            Some(("/tasks/search", "Search tasks")),
            Some(("/tasks/new", "Create task")),
        ),
        "/leases" => (
            "Leases",
            Some(("/leases/search", "Search leases")),
            Some(("/leases/new", "Create a lease")),
        ),
        "/leases/templates" => (
            "Leases",
            Some(("/leases/templates/search", "Search templates")),
            Some(("/leases/templates/new", "Create template")),
        ),
        _ => ("", None, None),
    };
    rsx! {
        header { class: if is_calendar { "mobile-app-topbar calendar-topbar can-compact" } else if title.is_empty() { "mobile-app-topbar is-hidden" } else if can_compact { "mobile-app-topbar can-compact" } else { "mobile-app-topbar" },
            if is_calendar {
                tour_pages::CalendarTopBar {}
            } else {
                h1 { "{title}" }
                if path == "/" {
                    CopyMcpPromptButton {}
                }
                if let Some((href, label)) = search {
                    a { class: "header-search-action", href, aria_label: label, title: label, Icon { name: "search" } }
                }
                if let Some((href, label)) = create {
                    a { class: "primary-button round-add-action", href, aria_label: label, title: label, Icon { name: "plus" } }
                }
            }
        }
    }
}

#[component]
fn CopyMcpPromptButton() -> Element {
    let mut copy_status = use_signal(String::new);
    let copied = copy_status() == "MCP instructions copied";
    rsx! {
        div { class: "mcp-copy-action",
            button {
                r#type: "button",
                class: "header-search-action",
                aria_label: "Copy Buildry MCP connection instructions",
                title: "Copy Buildry MCP connection instructions",
                onclick: move |_| {
                    spawn(async move {
                        let result = match web_sys::window() {
                            Some(window) => JsFuture::from(window.navigator().clipboard().write_text(MCP_CONNECTOR_PROMPT)).await,
                            None => Err(wasm_bindgen::JsValue::NULL),
                        };
                        copy_status.set(if result.is_ok() {
                            "MCP instructions copied".into()
                        } else {
                            "Could not copy instructions".into()
                        });
                    });
                },
                if copied { Icon { name: "check" } } else { Icon { name: "copy" } }
            }
            if !copy_status().is_empty() {
                span { class: "mcp-copy-status", role: "status", "{copy_status}" }
            }
        }
    }
}

#[component]
fn PricingPage() -> Element {
    return pricing_page::pricing_page();
}
#[component]
fn ToursPage() -> Element {
    return tour_pages::dashboard_page();
}
#[component]
fn VacancyPage() -> Element {
    return vacancy_page::vacancy_page();
}
#[component]
fn AccessPage() -> Element {
    return admin_pages::access_page();
}
#[component]
fn LandlordsPage() -> Element {
    return admin_pages::landlords_page();
}
#[component]
fn LeasesPage() -> Element {
    return admin_pages::leases_page();
}
#[component]
fn LeaseRegisterPage() -> Element {
    return admin_pages::lease_register_page();
}
#[component]
fn LeaseTemplatesPage() -> Element {
    return admin_pages::lease_templates_page();
}
#[component]
fn PropertyPage() -> Element {
    return property_page::property_page();
}
#[component]
fn CatalogPage() -> Element {
    return app(false);
}
#[component]
fn CatalogSearchPage() -> Element {
    return app(true);
}

fn app(is_search_page: bool) -> Element {
    let mut properties = use_signal(Vec::<Property>::new);
    let mut connection_status = use_signal(|| "Connecting to PostgreSQL…".to_string());
    let mut search = use_signal(String::new);
    use_effect(move || sidebar::set_property_count(properties().len()));
    use_effect(move || {
        spawn(async move {
            match api::bootstrap(&load_properties()).await {
                Ok(saved) => {
                    properties.set(saved);
                    connection_status.set("Saved in PostgreSQL".into());
                }
                Err(error) => connection_status.set(error),
            }
        });
    });

    let list = properties();
    let query = search().to_lowercase();
    let visible: Vec<Property> = list
        .iter()
        .filter(|p| {
            p.name.to_lowercase().contains(&query)
                || p.address.to_lowercase().contains(&query)
                || p.id.to_string().contains(&query)
        })
        .cloned()
        .collect();
    let visible_is_empty = visible.is_empty();
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

    if is_search_page {
        return rsx! {
            document::Title { "Search properties | Buildry" }
            document::Stylesheet { href: CSS }
            crate::admin_shell::AdminShell { active: "/", property_count: Some(list.len()), main_class: "main-content catalog-main search-page-main",
                searchable_list::SearchScreen { back_href: "/", placeholder: "Search properties", value: search(), count: visible.len(), on_search: move |value| search.set(value),
                    searchable_list::JointList { class: "catalog-panel",
                        div { class: "searchable-list-items property-list",
                            for property in visible {
                                a { key: "{property.id}", href: "/properties/{property.id}", class: "property-row register-row",
                                    img { class: "property-row-photo", src: property.photos.first().map(|photo| searchable_list::cover_thumbnail_url(photo)).unwrap_or_else(|| guest_pages::house_photo(property.id)), alt: "", loading: "lazy", decoding: "async" }
                                    span { class: "property-row-copy", strong { "{property.name}" } small { "{property.location_label()}" } }
                                    span { class: "property-row-arrow", aria_hidden: "true", Icon { name: "chevron-right" } }
                                }
                            }
                            if visible_is_empty { p { class: "empty", "No properties match that search." } }
                        }
                    }
                }
            }
        };
    }

    rsx! {
        document::Title { "Properties | Buildry" }
        document::Stylesheet { href: CSS }
        crate::admin_shell::AdminShell { active: "/", property_count: Some(list.len()), main_class: "main-content catalog-main",
            section { class: "page-head",
                div {
                    h1 { "Properties" }
                }
                div { class: "page-actions",
                    CopyMcpPromptButton {}
                    a { class: "header-search-action", href: "/properties/search", aria_label: "Search properties", title: "Search properties", Icon { name: "search" } }
                    a { class: "primary-button round-add-action", href: "/properties/new", aria_label: "Add property", title: "Add property", Icon { name: "plus" } span { "Add property" } }
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
            searchable_list::JointList { class: "catalog-panel",
                div { class: "searchable-list-items property-list",
                    for property in visible {
                        a { key: "{property.id}", href: "/properties/{property.id}", class: "property-row register-row",
                            img { class: "property-row-photo", src: property.photos.first().map(|photo| searchable_list::cover_thumbnail_url(photo)).unwrap_or_else(|| guest_pages::house_photo(property.id)), alt: "", loading: "lazy", decoding: "async" }
                            span { class: "property-row-copy", strong { "{property.name}" } small { "{property.location_label()}" } }
                            span { class: "property-row-arrow", aria_hidden: "true", Icon { name: "chevron-right" } }
                        }
                    }
                    if list.is_empty() { p { class: "empty", "No properties yet. Add your first house." } }
                    if !list.is_empty() && visible_is_empty { p { class: "empty", "No properties match that search." } }
                }
            }
        }
    }
}
