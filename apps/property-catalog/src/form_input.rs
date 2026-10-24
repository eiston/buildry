use dioxus::prelude::*;

/// Shared native data-entry control. Keep input semantics and validation in one place
/// while allowing each form to own its value and save behavior.
#[component]
pub fn FormInput(
    #[props(default)] r#type: Option<String>,
    #[props(default)] class: Option<String>,
    #[props(default)] id: Option<String>,
    #[props(default)] name: Option<String>,
    #[props(default)] aria_label: Option<String>,
    #[props(default)] placeholder: Option<String>,
    #[props(default)] value: Option<String>,
    #[props(default)] required: bool,
    #[props(default)] disabled: bool,
    #[props(default)] autofocus: bool,
    #[props(default)] maxlength: Option<String>,
    #[props(default)] minlength: Option<String>,
    #[props(default)] min: Option<String>,
    #[props(default)] max: Option<String>,
    #[props(default)] step: Option<String>,
    #[props(default)] autocomplete: Option<String>,
    #[props(default)] oninput: Option<EventHandler<FormEvent>>,
    #[props(default)] onchange: Option<EventHandler<FormEvent>>,
) -> Element {
    let class = format!("shared-form-input {}", class.unwrap_or_default());
    let input_type = r#type.unwrap_or_else(|| "text".into());
    let placeholder = if input_type == "search" {
        placeholder
    } else {
        Some("None".to_string())
    };
    if let Some(onchange) = onchange {
        rsx! { input {
            class, r#type: input_type, id, name, aria_label, placeholder, value,
            required, disabled, autofocus, maxlength, minlength, min, max, step, autocomplete,
            oninput: oninput.unwrap_or_else(|| EventHandler::new(|_: FormEvent| {})),
            onchange,
        } }
    } else {
        rsx! { input {
            class, r#type: input_type, id, name, aria_label, placeholder, value,
            required, disabled, autofocus, maxlength, minlength, min, max, step, autocomplete,
            oninput: oninput.unwrap_or_else(|| EventHandler::new(|_: FormEvent| {})),
        } }
    }
}

/// Shared native select for option lists that need the platform picker.
#[component]
pub fn FormNativeSelect(
    children: Element,
    #[props(default)] class: Option<String>,
    #[props(default)] value: Option<String>,
    #[props(default)] onchange: Option<EventHandler<FormEvent>>,
) -> Element {
    rsx! {
        select {
            class: format!("shared-form-select {}", class.unwrap_or_default()),
            value,
            onchange: onchange.unwrap_or_else(|| EventHandler::new(|_: FormEvent| {})),
            {children}
        }
    }
}

/// Shared upload control. The owning form handles the selected files.
#[component]
pub fn FormFileInput(
    accept: String,
    onchange: EventHandler<FormEvent>,
    #[props(default)] multiple: bool,
    #[props(default)] disabled: bool,
) -> Element {
    rsx! { input { class: "shared-form-file", r#type: "file", accept, multiple, disabled, onchange } }
}

/// Shared checkbox for form options and acknowledgements.
#[component]
pub fn FormCheckbox(checked: bool, onchange: EventHandler<FormEvent>) -> Element {
    rsx! { input { class: "shared-form-checkbox", r#type: "checkbox", checked, onchange } }
}
