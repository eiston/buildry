use crate::{form_input::FormInput, icon::Icon};
use dioxus::prelude::*;

#[derive(Clone, PartialEq)]
pub struct MultiSelectOption {
    pub value: String,
    pub label: String,
    pub description: String,
    pub image: String,
    pub disabled: bool,
}

impl MultiSelectOption {
    pub fn new(value: String, label: String, image: String) -> Self {
        Self {
            value,
            label,
            image,
            description: String::new(),
            disabled: false,
        }
    }
}

fn initials(label: &str) -> String {
    label
        .split_whitespace()
        .take(2)
        .filter_map(|word| word.chars().next())
        .collect::<String>()
        .to_uppercase()
}

/// Shared searchable person selector for single or multiple choices.
#[component]
pub fn FormMultiSelect(
    label: String,
    selected: Vec<String>,
    options: Vec<MultiSelectOption>,
    onchange: EventHandler<Vec<String>>,
    #[props(default)] class: Option<String>,
    #[props(default)] placeholder: Option<String>,
    #[props(default)] search_placeholder: Option<String>,
    #[props(default)] icon: Option<String>,
    #[props(default)] show_done: bool,
    #[props(default)] show_selected_names: bool,
    #[props(default)] single_select: bool,
) -> Element {
    let mut open = use_signal(|| false);
    let mut search = use_signal(String::new);
    let count = selected.len();
    let selected_options = options
        .iter()
        .filter(|item| selected.contains(&item.value))
        .cloned()
        .collect::<Vec<_>>();
    let current = match count {
        0 => placeholder.unwrap_or_else(|| "None".to_string()),
        1 => options
            .iter()
            .find(|item| selected.contains(&item.value))
            .map(|item| item.label.clone())
            .unwrap_or_else(|| label.clone()),
        _ if show_selected_names => options
            .iter()
            .filter(|item| selected.contains(&item.value))
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        _ => format!("{} · {}", label, count),
    };
    let class = class.unwrap_or_default();
    let icon_name = if icon.as_deref() == Some("house") {
        "house"
    } else {
        "users"
    };
    rsx! {
        div { class: format!("form-multiselect {} {} {}", class, if count > 0 || open() { "is-active" } else { "" }, if count == 0 { "is-empty" } else { "" }),
            button { r#type: "button", class: "form-multiselect-trigger", aria_label: "{label}: {current}", aria_expanded: open(), aria_haspopup: "listbox", onclick: move |_| open.set(!open()),
                if show_selected_names && !selected_options.is_empty() {
                    span { class: "form-multiselect-people",
                        for person in selected_options {
                            span { key: "{person.value}", class: "form-multiselect-person",
                                span { class: "form-multiselect-avatar",
                                    if person.image.is_empty() { "{initials(&person.label)}" } else { img { src: "{person.image}", alt: "" } }
                                }
                                span { class: "form-multiselect-person-name", "{person.label}" }
                            }
                        }
                    }
                } else {
                    if let Some(first) = options.iter().find(|item| selected.contains(&item.value)) {
                        span { class: "form-multiselect-avatar",
                            if first.image.is_empty() { "{initials(&first.label)}" } else { img { src: "{first.image}", alt: "" } }
                        }
                    } else if !show_selected_names && !single_select { Icon { name: icon_name } }
                    span { class: "form-multiselect-value", "{current}" }
                    if count > 1 && !show_selected_names { span { class: "form-multiselect-count", "{count}" } }
                }
                Icon { name: "chevron-down" }
            }
            if open() {
                button { r#type: "button", class: "form-multiselect-dismiss", aria_label: "Close {label} options", onclick: move |_| open.set(false) }
                div { class: "form-multiselect-menu", role: "listbox", aria_label: "{label}", aria_multiselectable: !single_select,
                    div { class: "form-multiselect-head", strong { "{label}" }
                        if count > 0 { button { r#type: "button", onclick: move |_| onchange.call(vec![]), "Clear" } }
                        if show_done && !single_select { button { r#type: "button", onclick: move |_| open.set(false), "Done" } }
                    }
                    if let Some(search_placeholder) = search_placeholder {
                        FormInput { r#type: "search", aria_label: "Search {label}", placeholder: search_placeholder, value: "{search}", oninput: move |event: FormEvent| search.set(event.value()) }
                    }
                    for item in options.into_iter().filter(|item| item.label.to_lowercase().contains(&search().to_lowercase())) {
                        button { key: "{item.value}", r#type: "button", role: "option", class: "form-multiselect-option", aria_selected: selected.contains(&item.value), disabled: item.disabled,
                            onclick: { let value = item.value.clone(); let selected = selected.clone(); move |_| {
                                if single_select {
                                    onchange.call(vec![value.clone()]);
                                    open.set(false);
                                    search.set(String::new());
                                } else {
                                    let mut next = selected.clone();
                                    if next.contains(&value) { next.retain(|existing| existing != &value); } else { next.push(value.clone()); }
                                    onchange.call(next);
                                }
                            } },
                            span { class: "form-multiselect-avatar", if item.image.is_empty() { "{initials(&item.label)}" } else { img { src: "{item.image}", alt: "" } } }
                            span { class: "form-multiselect-option-copy", strong { "{item.label}" } if !item.description.is_empty() { small { "{item.description}" } } }
                            span { class: if selected.contains(&item.value) { "form-multiselect-check selected" } else { "form-multiselect-check" }, if selected.contains(&item.value) { Icon { name: "check" } } }
                        }
                    }
                }
            }
        }
    }
}
