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
    #[props(default)] inputmode: Option<String>,
    #[props(default)] oninput: Option<EventHandler<FormEvent>>,
    #[props(default)] onchange: Option<EventHandler<FormEvent>>,
) -> Element {
    let field = try_use_context::<crate::form_select::FieldContext>();
    let required = required || field.as_ref().is_some_and(|field| field.required);
    let labelled_by = if aria_label.is_none() { field.map(|field| field.label_id) } else { None };
    let class = format!("shared-form-input {}", class.unwrap_or_default());
    let input_type = r#type.unwrap_or_else(|| "text".into());
    if let Some(onchange) = onchange {
        rsx! { input {
            class, r#type: input_type, id, name, aria_label, aria_labelledby: labelled_by, placeholder, value,
            required, disabled, autofocus, maxlength, minlength, min, max, step, autocomplete, inputmode,
            oninput: oninput.unwrap_or_else(|| EventHandler::new(|_: FormEvent| {})),
            onchange,
        } }
    } else {
        rsx! { input {
            class, r#type: input_type, id, name, aria_label, aria_labelledby: labelled_by, placeholder, value,
            required, disabled, autofocus, maxlength, minlength, min, max, step, autocomplete, inputmode,
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

/// Email validation for actions outside a native form submission.
pub(crate) fn is_valid_email(value: &str) -> bool {
    let Some((local, domain)) = value.split_once('@') else { return false; };
    !local.is_empty() && local.len() <= 64 && value.len() <= 254
        && local.bytes().all(|ch| ch.is_ascii_alphanumeric() || b".!#$%&'*+/=?^_`{|}~-".contains(&ch))
        && domain.split('.').all(|label| {
            !label.is_empty() && label.len() <= 63
                && label.as_bytes()[0].is_ascii_alphanumeric()
                && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                && label.bytes().all(|ch| ch.is_ascii_alphanumeric() || ch == b'-')
        })
}

#[cfg(test)]
mod email_tests {
    use super::is_valid_email;

    #[test]
    fn email_actions_reject_incomplete_or_malformed_addresses() {
        for email in ["", "guest", "guest@", "@example.test", "a@@example.test", "a b@example.test", "guest@-example.test", "guest@example..test"] {
            assert!(!is_valid_email(email), "{email}");
        }
        for email in ["guest@example.test", "guest+room@example.test", "guest@sub.example.test", "guest@example.invalid"] {
            assert!(is_valid_email(email), "{email}");
        }
    }
}
