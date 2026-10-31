use crate::icon::Icon;
use dioxus::prelude::*;

#[component]
pub fn FormField(label: &'static str, required: bool, children: Element) -> Element {
    rsx! { div { class: "form-field", div { class: "form-field-heading", span { "{label}" if required { span { class: "form-required-marker", aria_label: "required", " *" } } } } {children} } }
}

#[component]
pub fn FormSelect(
    label: &'static str,
    required: bool,
    value: String,
    placeholder: &'static str,
    options: Vec<(String, String)>,
    onchange: EventHandler<String>,
    #[props(default)] disabled_values: Vec<String>,
    #[props(default)] show_avatars: bool,
    #[props(default)] avatar_initials_fallback: bool,
    #[props(default)] option_images: Vec<(String, String)>,
    #[props(default)] compact: bool,
    #[props(default)] icon: Option<&'static str>,
    #[props(default)] display_override: Option<String>,
) -> Element {
    let mut open = use_signal(|| false);
    let empty =
        !compact && (value.trim().is_empty() || !options.iter().any(|(key, _)| key == &value));
    let current = if empty {
        "None".to_string()
    } else {
        display_override.unwrap_or_else(|| {
            options
                .iter()
                .find(|(key, _)| key == &value)
                .map(|(_, text)| text.clone())
                .unwrap_or_else(|| placeholder.into())
        })
    };
    let control = rsx! {
        div { class: if compact { "form-select compact housework-filter-select" } else if empty { "form-select is-empty" } else { "form-select" },
            button { r#type: "button", class: if open() { "form-select-button open" } else { "form-select-button" },
                aria_label: label, aria_expanded: open(), aria_haspopup: "listbox",
                onclick: move |_| open.set(!open()),
                if let Some(icon) = icon { Icon { name: icon } }
                if show_avatars && !empty { span { class: if avatar_initials_fallback { "form-select-avatar portrait" } else { "form-select-avatar" },
                    if let Some((_, image)) = option_images.iter().find(|(key, _)| key == &value) {
                        if image.is_empty() { if avatar_initials_fallback { Icon { name: "users" } } else { Icon { name: "house" } } } else { img { src: "{image}", alt: "" } }
                    } else if avatar_initials_fallback { Icon { name: "users" } } else { Icon { name: "house" } }
                } }
                span { class: "form-select-value", "{current}" }
                span { class: "form-select-chevron", aria_hidden: "true", Icon { name: "chevron-down" } }
            }
            if open() {
                button { r#type: "button", class: "form-select-dismiss", aria_label: "Close {label} options", onclick: move |_| open.set(false) }
                div { class: "form-select-menu", role: "listbox", aria_label: label,
                for (key, text) in options {
                    button { key: "{key}", r#type: "button", role: "option", aria_selected: value == key,
                        disabled: disabled_values.contains(&key), class: if value == key { "selected" } else { "" },
                        onclick: move |_| { onchange.call(key.clone()); open.set(false); },
                        if show_avatars { span { class: if avatar_initials_fallback { "form-select-avatar portrait" } else { "form-select-avatar" },
                            if let Some((_, image)) = option_images.iter().find(|(image_key, _)| image_key == &key) {
                                if image.is_empty() { if avatar_initials_fallback { Icon { name: "users" } } else { Icon { name: "house" } } } else { img { src: "{image}", alt: "" } }
                            } else if avatar_initials_fallback { Icon { name: "users" } } else { Icon { name: "house" } }
                        } }
                        span { "{text}" }
                    }
                }
                }
            }
        }
    };
    if compact {
        control
    } else {
        rsx! { FormField { label, required, {control} } }
    }
}
