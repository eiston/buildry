mod workspace;
mod time_picker;
mod shared_ui;
mod tour_time_picker;
mod schedule_summary;
mod photo_gallery;
use icon::Icon;
mod address_picker;
mod admin_pages;
mod admin_shell;
mod api;
mod app_form;
mod collapsible_panel;
mod date_picker;
mod form_action_bar;
mod form_input;
mod form_button;
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
mod payment_history;
mod payments;
mod payout_setup;
mod rent_pad;
mod tour_api;
mod tour_model;
mod tour_pages;
mod vacancy_page;
mod upload_dropzone;

use dioxus::prelude::*;
use model::Property;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

static CSS: Asset = asset!("/assets/main.css");
static ICON_RIPPLE: Asset = asset!("/assets/icon-ripple.mjs");
static GALLERY_SORT: Asset = asset!("/assets/gallery-sort.js");
static PORTFOLIO_MAP: Asset = asset!("/assets/portfolio-map.html");
const MCP_CONNECTOR_PROMPT: &str = "Please add Buildry to your MCP connectors as a remote HTTP MCP server at https://app.buildry.ca/mcp. Authenticate with my Buildry account using the OAuth flow when prompted. Request the listings.read, tasks.read, and tasks.write scopes so you can access property and room listings, view tasks, and create tasks when I ask. After connecting, discover the available Buildry tools and use them for my requests.";

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
        | "/tours"
        | "/vacancy"
        | "/access"
        | "/access/search"
        | "/access/new"
        | "/access/view"
        | "/landlords"
        | "/landlords/new"
        | "/landlords/view"
        | "/leases"
        | "/leases/search"
        | "/leases/new"
        | "/leases/view"
        | "/leases/payments"
        | "/leases/deposit-setup"
        | "/leases/templates"
        | "/leases/templates/search"
        | "/leases/templates/new" => dioxus::launch(AdminApp),
        "/my-home" => dioxus::launch(AdminApp),
        "/my-home/payments" | "/my-home/payments/history" => dioxus::launch(AdminApp),
        "/my-home/account" => dioxus::launch(AdminApp),
        path if path.starts_with("/my-home/requests") => {
            dioxus::launch(AdminApp)
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

    let mut session_error=use_signal(String::new);
    use_effect(move || { spawn(async move { match workspace::get("/api/me").await { Ok(value)=>workspace::USER.signal().set(Some(value)),Err(e)=>session_error.set(e) } }); });
    let Some(user)=workspace::USER() else { return rsx! { document::Stylesheet { href:CSS } main {class:"main-content", if session_error().is_empty(){p{role:"status","Loading account…"}}else{p{role:"alert","{session_error}"}a{href:"/login","Sign in"}} } }; };
    if user["onboarding_completed"] != true { return rsx! {document::Stylesheet{href:CSS} workspace::Onboarding{}}; }
    let location = route();
    let path = location.split('?').next().unwrap_or("/").to_string();
    if path=="/" || path=="/onboarding" { let installed=workspace::strings(&user,"installed_apps"); let dest=workspace::APPS.iter().find(|a|Some(a.0)==installed.first().map(String::as_str)).map(|a|a.2).unwrap_or("/apps"); workspace::navigate(dest); return rsx!{}; }
    let active = match path.as_str() {
        "/apps"=>"/apps", "/contacts"=>"/contacts",
        path if path.starts_with("/account")=>"/account",
        path if path.starts_with("/my-rentals")=>"/my-rentals",
        path if path.starts_with("/my-home/requests")=>"/tasks",
        path if path.starts_with("/payments")=>"/payments",
        "/calendar" | "/tours" => "/calendar",
        path if path.starts_with("/tasks") => "/tasks",
        "/pricing" => "/pricing",
        "/vacancy" => "/vacancy",
        path if path.starts_with("/leases") => "/leases",
        path if path.starts_with("/access") => "/access",
        path if path.starts_with("/landlords")=>"/leases",
        _ => "/properties",
    };
    let page = match path.as_str() {
        "/apps"=>rsx!{workspace::AppsPage{}},
        "/contacts"=>rsx!{workspace::ContactsPage{}},
        "/my-rentals"|"/my-home"=>rsx!{workspace::RentalsPage{}},
        path if path.starts_with("/account")||path=="/my-home/account"=>rsx!{workspace::AccountPage{key:"{location}",path:path.to_string()}},
        path if path.starts_with("/payments")=>rsx!{payments::PaymentsApp{key:"{location}"}},
        path if path.starts_with("/my-home/requests")=>rsx!{tenants_page::maintenance_page{}},
        path if path.starts_with("/sharing/")=>{let parts:Vec<_>=path.split('/').collect();rsx!{workspace::SharingPage{key:"{location}",kind:parts.get(2).unwrap_or(&"").to_string(),id:parts.get(3).and_then(|v|v.parse::<i64>().ok()).unwrap_or(0)}}},
        "/properties"=>rsx!{CatalogPage{}},
        "/pricing" => rsx! { PricingPage { key: "{location}" } },
        "/calendar" | "/tours" => rsx! { ToursPage { key: "{location}" } },
        path if path.starts_with("/tasks") => {
            rsx! { housework_page::page { key: "{location}" } }
        }
        "/vacancy" => rsx! { VacancyPage { key: "{location}" } },
        "/access" | "/access/search" | "/access/new" | "/access/view" => {
            rsx! { AccessPage { key: "{location}" } }
        }
        "/landlords" | "/landlords/new" | "/landlords/view" => {
            rsx! { LandlordsPage { key: "{location}" } }
        }
        "/leases" | "/leases/templates" => rsx! { LeaseRegisterPage { key: "{location}" } },
        "/leases/payments" => rsx! { admin_pages::LeasePaymentHistoryPage { key: "{location}" } },
        "/leases/deposit-setup" => rsx! { admin_pages::DepositSetupPage { key: "{location}" } },
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
        a { class: "skip-link", href: "#main-content", "Skip to content" }
        div { class: "app-shell",
            sidebar::Sidebar { active }
            MobileTopBar { path: path.clone() }
            div{class:"workspace-page",if active=="/leases"{workspace::SignatureInbox{}} {page}}
        }
    }
}

#[component]
fn MobileTopBar(path: String) -> Element {
    let is_calendar = matches!(path.as_str(), "/calendar" | "/tours");
    let can_compact = matches!(
        path.as_str(),
        "/" | "/properties" | "/calendar" | "/tours" | "/tasks" | "/leases" | "/leases/templates"
    );
    let (title, search, create) = match path.as_str() {
        "/" | "/properties" => (
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
                if path == "/" || path=="/properties" {
                    CopyMcpPromptButton {}
                }
                if let Some((href, label)) = search {
                    a { class: "header-search-action", href, aria_label: label, title: label, Icon { name: "search" } }
                }
                if let Some((href, label)) = create.filter(|_| path != "/" || sidebar::can_create_resources()) {
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
    rsx!{workspace::LandlordsPage{}}
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
    let mut connection_status = use_signal(|| "Loading properties…".to_string());
    let mut search = use_signal(String::new);
    use_effect(move || sidebar::set_property_count(properties().len()));
    use_effect(move || {
        spawn(async move {
            let result = api::list().await;
            match result {
                Ok(saved) => {
                    properties.set(saved);
                    connection_status.set("Saved".into());
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
            crate::admin_shell::AdminShell { active: "/properties", property_count: Some(list.len()), main_class: "main-content catalog-main search-page-main",
                searchable_list::SearchScreen { back_href: "/properties", placeholder: "Search properties", value: search(), count: visible.len(), on_search: move |value| search.set(value),
                    searchable_list::JointList { class: "catalog-panel",
                        div { class: "searchable-list-items property-list",
                            for property in visible {
                                searchable_list::ThumbnailListRow { key: "{property.id}", href: format!("/properties/{}", property.id), name: property.name.clone(), subtitle: property.location_label(), photo: property.photos.first().cloned() }
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
        crate::admin_shell::AdminShell { active: "/properties", property_count: Some(list.len()), main_class: "main-content catalog-main",
            section { class: "page-head",
                div {
                    h1 { "Properties" }
                }
                div { class: "page-actions",
                    CopyMcpPromptButton {}
                    a { class: "header-search-action", href: "/properties/search", aria_label: "Search properties", title: "Search properties", Icon { name: "search" } }
                    if sidebar::can_create_resources() { a { class: "primary-button round-add-action", href: "/properties/new", aria_label: "Add property", title: "Add property", Icon { name: "plus" } span { "Add property" } } }
                }
            }
            if connection_status() != "Saved" { p { class: "storage-status", "{connection_status}" } }
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
                        searchable_list::ThumbnailListRow { key: "{property.id}", href: format!("/properties/{}", property.id), name: property.name.clone(), subtitle: property.location_label(), photo: property.photos.first().cloned() }
                    }
                    if list.is_empty() { p { class: "empty", if sidebar::can_create_resources() { "No properties yet. Add your first house." } else { "No properties are assigned to this account." } } }
                    if !list.is_empty() && visible_is_empty { p { class: "empty", "No properties match that search." } }
                }
            }
        }
    }
}
