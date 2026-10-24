use crate::icon::Icon;
use dioxus::prelude::*;

pub fn cover_thumbnail_url(url: &str) -> String {
    if url.starts_with("/images/") {
        format!("{url}{}thumb=1", if url.contains('?') { '&' } else { '?' })
    } else {
        url.to_string()
    }
}

/// The shared search and rows used by management lists.
#[component]
pub fn SearchableList(
    class: &'static str,
    search_placeholder: &'static str,
    search_value: String,
    on_search: EventHandler<String>,
    #[props(default = true)] show_search: bool,
    children: Element,
) -> Element {
    rsx! {
        section { class: "searchable-list {class}",
            if show_search {
                div { class: "searchable-list-search-card",
                    div { class: "searchable-list-controls",
                        crate::form_input::FormInput {
                            r#type: "search",
                            aria_label: search_placeholder,
                            placeholder: search_placeholder,
                            value: "{search_value}",
                            oninput: move |event: FormEvent| on_search.call(event.value()),
                        }
                    }
                }
            }
            div { class: "searchable-list-results-card", {children} }
        }
    }
}

/// Shared card for register rows, with optional controls above the rows.
#[component]
pub fn JointList(
    class: &'static str,
    #[props(default)] filters: Option<Element>,
    children: Element,
) -> Element {
    rsx! {
        section { class: "joint-list {class}",
            if let Some(filters) = filters {
                div { class: "joint-list-filters", {filters} }
            }
            div { class: "joint-list-rows", {children} }
        }
    }
}

#[component]
pub fn SearchScreen(
    back_href: &'static str,
    placeholder: &'static str,
    value: String,
    count: usize,
    on_search: EventHandler<String>,
    children: Element,
) -> Element {
    let caption = if value.trim().is_empty() {
        "Browse all".to_string()
    } else {
        format!("{count} result{}", if count == 1 { "" } else { "s" })
    };
    rsx! {
        div { class: "search-screen",
            div { class: "search-screen-bar",
                a { class: "back-icon-button", href: back_href, aria_label: "Back", title: "Back", Icon { name: "arrow-left" } }
                crate::form_input::FormInput { r#type: "search", aria_label: placeholder, placeholder: placeholder, value: "{value}", autofocus: true, oninput: move |event: FormEvent| on_search.call(event.value()) }
            }
            p { class: "search-screen-caption", "{caption}" }
            div { class: "search-screen-results", {children} }
        }
    }
}
