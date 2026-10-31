use crate::icon::Icon;
use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};

static ACCESS_ALLOWED: GlobalSignal<bool> = Signal::global(|| false);
static PROPERTY_COUNT: GlobalSignal<Option<usize>> = Signal::global(|| None);
static COLLAPSED: GlobalSignal<bool> =
    Signal::global(|| LocalStorage::get::<bool>("buildry.sidebar.collapsed").unwrap_or(false));

pub fn set_property_count(count: usize) {
    PROPERTY_COUNT.signal().set(Some(count));
}

pub fn is_admin() -> bool {
    ACCESS_ALLOWED.signal()()
}

#[component]
pub fn Sidebar(active: &'static str) -> Element {
    let mut route = use_context::<Signal<String>>();
    let mut access_allowed = ACCESS_ALLOWED.signal();
    use_effect(move || {
        spawn(async move {
            if let Ok(response) = gloo_net::http::Request::get("/api/me").send().await {
                if let Ok(value) = response.json::<serde_json::Value>().await {
                    access_allowed.set(value["role"] == "admin");
                }
            }
        });
    });
    let mut collapsed = COLLAPSED.signal();
    let items = [
        ("/", "Properties", "Houses", "icon-properties"),
        ("/pricing", "Pricing", "Pricing", "icon-pricing"),
        ("/calendar", "Calendar", "Calendar", "icon-tours"),
        ("/tasks", "Tasks", "Tasks", "icon-housework"),
        ("/vacancy", "Vacancy", "Vacancy", "icon-availability"),
        ("/leases", "Leases", "Leases", "icon-leases"),
        ("/access", "Access", "Access", "icon-access"),
    ];

    rsx! {
        aside { class: if collapsed() { "sidebar collapsed" } else { "sidebar" },
            div { class: "sidebar-header",
                div { class: "brand",
                    img { class: "brand-symbol", src: "/icons/buildry-mark.svg?v=20261017-vibrant", alt: "" }
                    span { class: "brand-name", "Buildry" }
                }
                button { class: "sidebar-toggle", r#type: "button",
                    aria_label: if collapsed() { "Expand sidebar" } else { "Collapse sidebar" },
                    title: if collapsed() { "Expand sidebar" } else { "Collapse sidebar" },
                    onclick: move |_| {
                        let next = !collapsed();
                        collapsed.set(next);
                        let _ = LocalStorage::set("buildry.sidebar.collapsed", next);
                    },
                    if collapsed() { Icon { name: "chevron-right" } } else { Icon { name: "chevron-left" } }
                }
            }
            for (path, label, mobile_label, icon) in items {if path != "/access" || access_allowed() {
                a { key: "{path}", class: format!("nav-item tour-nav-link{}{}", if active == path { " active" } else { "" }, if matches!(path, "/pricing" | "/vacancy" | "/access") { " nav-mobile-hidden" } else { "" }),
                    href: path, title: label,
                    onclick: move |event| {
                        if !event.modifiers().is_empty() {
                            return;
                        }
                        event.prevent_default();
                        if route() != path {
                            if let Some(window) = web_sys::window() {
                                if let Ok(history) = window.history() {
                                    let _ = history.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(path));
                                }
                            }
                            route.set(path.to_string());
                        }
                    },
                    span { class: "nav-icon {icon}", aria_hidden: "true" }
                    span { class: "nav-text nav-text-desktop", "{label}" }
                    span { class: "nav-text nav-text-mobile", "{mobile_label}" }
                    if path == "/" {
                        if let Some(count) = PROPERTY_COUNT() { span { class: "nav-count", "{count}" } }
                    }
                }
            }}
            div { class: "nav-section-divider", role: "separator" }
            a { class: "nav-item tour-nav-link nav-public-link", href: "/stays", target: "_blank",
                rel: "noopener noreferrer", title: "Open guest site in a new tab",
                span { class: "nav-icon icon-stays", aria_hidden: "true" }
                span { class: "nav-text", span { "Guest site" } Icon { name: "arrow-up-right" } }
            }
            crate::app_form::AppForm { class: "nav-logout", method: "post", action: "/logout",
                button { class: "nav-item tour-nav-link", r#type: "submit", title: "Sign out",
                    span { class: "nav-icon icon-logout", aria_hidden: "true" }
                    span { class: "nav-text", "Sign out" }
                }
            }
        }
    }
}
