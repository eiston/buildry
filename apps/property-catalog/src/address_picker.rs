use crate::{icon::Icon, map, searchable_list::JointList, CSS};
use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

#[derive(Clone, PartialEq)]
pub struct AddressSelection {
    pub label: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[component]
pub fn AddressField(label: String, value: String, onopen: EventHandler<MouseEvent>) -> Element {
    let empty = value.trim().is_empty();
    rsx! {
        div { class: "address-field",
            span { class: "inline-field-label", "{label}" }
            button {
                class: if empty { "address-search-trigger is-empty" } else { "address-search-trigger" },
                r#type: "button",
                aria_label: "Search {label}",
                onclick: move |event| onopen.call(event),
                span { if empty { "Search for an address" } else { "{value}" } }
            }
        }
    }
}

fn search_addresses(
    value: String,
    mut query: Signal<String>,
    mut suggestions: Signal<Vec<map::AddressSuggestion>>,
    mut search_id: Signal<u32>,
    mut searching: Signal<bool>,
    mut error: Signal<String>,
) {
    query.set(value.clone());
    suggestions.set(Vec::new());
    error.set(String::new());
    search_id += 1;
    let current_id = search_id();
    if value.trim().len() < 4 {
        searching.set(false);
        return;
    }
    searching.set(true);
    spawn(async move {
        TimeoutFuture::new(450).await;
        if search_id() != current_id {
            return;
        }
        let result = map::suggest_addresses(&value).await;
        if search_id() != current_id {
            return;
        }
        searching.set(false);
        match result {
            Ok(matches) => suggestions.set(matches),
            Err(reason) => error.set(reason),
        }
    });
}

#[component]
pub fn AddressSearchScreen(
    initial_value: String,
    onselect: EventHandler<AddressSelection>,
    onback: EventHandler<MouseEvent>,
    #[props(default)] overlay: bool,
) -> Element {
    let query = use_signal(|| initial_value.clone());
    let suggestions = use_signal(Vec::<map::AddressSuggestion>::new);
    let mut search_id = use_signal(|| 0_u32);
    let searching = use_signal(|| false);
    let error = use_signal(String::new);

    use_effect(move || {
        let value = initial_value.clone();
        spawn(async move {
            search_addresses(value, query, suggestions, search_id, searching, error);
        });
    });

    let typed_address = query().trim().to_string();
    let matches = suggestions();
    let caption = if typed_address.len() < 4 {
        "Type at least 4 characters to find an address.".to_string()
    } else if matches.is_empty() && error().is_empty() {
        "No suggested addresses found.".to_string()
    } else {
        format!(
            "{} suggested address{}",
            matches.len(),
            if matches.len() == 1 { "" } else { "es" }
        )
    };
    rsx! {
        document::Title { "Search address | Buildry" }
        document::Stylesheet { href: CSS }
        main { class: if overlay { "main-content search-page-main address-search-main address-search-overlay" } else { "main-content search-page-main address-search-main" },
            div { class: "search-screen",
                div { class: "search-screen-bar",
                    button { class: "back-icon-button", r#type: "button", aria_label: "Back to form", title: "Back to form", onclick: move |event| { search_id += 1; onback.call(event); }, Icon { name: "arrow-left" } }
                    crate::form_input::FormInput { r#type: "search", aria_label: "Search street address", placeholder: "Search street address", value: query(), autofocus: true,
                        oninput: move |event: FormEvent| search_addresses(event.value(), query, suggestions, search_id, searching, error)
                    }
                }
                if !searching() { p { class: "search-screen-caption", role: "status", "{caption}" } }
                div { class: if searching() { "search-screen-results is-loading" } else { "search-screen-results" },
                    if searching() {
                        div { class: "address-loading-state", role: "status", span { "Finding addresses…" } }
                    } else {
                        JointList { class: "address-results",
                            if !error().is_empty() { p { class: "address-result-message", role: "alert", "{error}" } }
                            for suggestion in matches {
                                button { class: "address-result-row", r#type: "button", onclick: move |_| {
                                    search_id += 1;
                                    onselect.call(AddressSelection { label: suggestion.label.clone(), latitude: Some(suggestion.latitude), longitude: Some(suggestion.longitude) });
                                }, Icon { name: "search" }, span { "{suggestion.label}" } }
                            }
                            if !typed_address.is_empty() {
                                button { class: "address-result-row address-manual-row", r#type: "button", onclick: move |_| {
                                    search_id += 1;
                                    onselect.call(AddressSelection { label: typed_address.clone(), latitude: None, longitude: None });
                                }, Icon { name: "plus" }, span { "Use “{typed_address}” as entered" } }
                            }
                        }
                    }
                }
            }
        }
    }
}
