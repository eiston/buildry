use crate::{form_action_bar::FormActionBar, icon::Icon};
use dioxus::prelude::*;

/// A form field that edits its value on a full-screen writing surface.
/// Changes are handed to the owning form only when Save is pressed.
#[component]
pub fn NotebookTextarea(
    label: String,
    value: String,
    onchange: EventHandler<String>,
    #[props(default)] placeholder: Option<String>,
    #[props(default)] maxlength: Option<usize>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut draft = use_signal(String::new);
    let mut history = use_signal(Vec::<String>::new);
    let mut position = use_signal(|| 0usize);
    let hint = placeholder.unwrap_or_else(|| format!("Add {label}…"));
    let preview = if value.trim().is_empty() {
        "None".to_string()
    } else {
        value.clone()
    };
    let empty = value.trim().is_empty();

    rsx! {
        button { r#type: "button", class: "notebook-field", aria_label: "Edit {label}", onclick: {
            let value = value.clone();
            move |_| {
                draft.set(value.clone());
                history.set(vec![value.clone()]);
                position.set(0);
                open.set(true);
            }
        },
            span { class: if empty { "notebook-field-preview is-empty" } else { "notebook-field-preview" }, "{preview}" }
        }
        if open() {
            div { class: "notebook-screen", role: "dialog", aria_modal: "true", aria_label: "Edit {label}",
                FormActionBar {
                    back_href: String::new(),
                    back_label: "Back without saving".to_string(),
                    on_back: move |_| open.set(false),
                    center: Some(rsx! {
                        button { r#type: "button", class: "form-action-icon", aria_label: "Undo", title: "Undo", disabled: position() == 0,
                            onclick: move |_| { let next = position().saturating_sub(1); if let Some(text) = history().get(next) { draft.set(text.clone()); position.set(next); } }, Icon { name: "undo" } }
                        button { r#type: "button", class: "form-action-icon", aria_label: "Redo", title: "Redo", disabled: position() + 1 >= history().len(),
                            onclick: move |_| { let next = position() + 1; if let Some(text) = history().get(next) { draft.set(text.clone()); position.set(next); } }, Icon { name: "redo" } }
                    }),
                    button { r#type: "button", class: "property-save-button", aria_label: "Save {label}", title: "Save {label}", onclick: move |_| { onchange.call(draft()); open.set(false); }, Icon { name: "check" } }
                }
                textarea { class: "notebook-page", aria_label: "{label}", placeholder: "{hint}", value: "{draft}", maxlength: maxlength.map(|n| n.to_string()), autofocus: true,
                    oninput: move |event| {
                        let next = event.value();
                        if next == draft() { return; }
                        let mut entries = history();
                        entries.truncate(position() + 1);
                        entries.push(next.clone());
                        let last = entries.len() - 1;
                        history.set(entries);
                        position.set(last);
                        draft.set(next);
                    }
                }
            }
        }
    }
}
