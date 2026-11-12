mod components;
mod layout;
mod pages;
mod services;

use buildry_models::{guest_model, housework_model, model, tour_model};
use layout::{mobile_top_bar::MobileTopBar, sidebar};
use pages::{
    admin_pages, guest_pages, housework_page, payments, pricing_page, property_page,
    tenants_page, tour_pages, vacancy_page, workspace,
    catalog_page::{CatalogPage, CatalogSearchPage},
};

use dioxus::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

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
            {page}
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
