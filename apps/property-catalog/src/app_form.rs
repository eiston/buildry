use dioxus::prelude::*;

/// Shared form root for validation, submit handling, and form-level styling.
#[component]
pub fn AppForm(
    children: Element,
    #[props(default)] id: Option<String>,
    #[props(default)] class: Option<String>,
    #[props(default)] method: Option<String>,
    #[props(default)] action: Option<String>,
    #[props(default)] onsubmit: Option<EventHandler<FormEvent>>,
) -> Element {
    let class = format!("app-form {}", class.unwrap_or_default());
    if let Some(onsubmit) = onsubmit {
        rsx! { form { id, class, method, action, onsubmit, {children} } }
    } else {
        rsx! { form { id, class, method, action, {children} } }
    }
}

/// Full-width fields with the property form's spacing and row highlight.
#[component]
pub fn FormRows(children: Element, #[props(default)] class: Option<String>) -> Element {
    rsx! { div { class: format!("shared-form-rows {}", class.unwrap_or_default()), {children} } }
}
