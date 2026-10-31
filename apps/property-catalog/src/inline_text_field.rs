use dioxus::prelude::*;

/// Readable field chrome that turns into a native input when activated.
/// The owning form decides how and when to persist the committed value.
#[component]
pub fn InlineTextField(
    label: String,
    value: String,
    oncommit: EventHandler<String>,
    #[props(default)] oninput: Option<EventHandler<String>>,
    #[props(default)] placeholder: Option<String>,
    #[props(default)] maxlength: Option<usize>,
    #[props(default)] input_type: Option<String>,
    #[props(default)] min: Option<String>,
    #[props(default)] autocomplete: Option<String>,
) -> Element {
    let mut editing = use_signal(|| false);
    let mut draft = use_signal(String::new);
    let mut original = use_signal(String::new);
    let empty = value.trim().is_empty();
    let display = if empty {
        "None".to_string()
    } else {
        value.clone()
    };

    rsx! {
        div { class: "inline-field", onclick: {
            let value = value.clone();
            move |_| {
                if !editing() {
                    draft.set(value.clone());
                    original.set(value.clone());
                    editing.set(true);
                }
            }
        },
            span { class: "inline-field-label", "{label}" }
            if editing() {
                input { class: "inline-field-input", aria_label: "{label}", r#type: input_type.unwrap_or_else(|| "text".into()), min: min, autocomplete: autocomplete, placeholder: placeholder, value: "{draft}", maxlength: maxlength.map(|n| n.to_string()), autofocus: true,
                    onmounted: move |event: MountedEvent| { spawn(async move { let _ = event.data().set_focus(true).await; }); },
                    oninput: move |event: FormEvent| { let text=event.value(); draft.set(text.clone()); if let Some(handler)=oninput { handler.call(text); } },
                    onblur: {
                        move |_| {
                            if !editing() { return; }
                            editing.set(false);
                            let next = draft();
                            if next != original() { oncommit.call(next); }
                        }
                    },
                    onkeydown: move |event: KeyboardEvent| match event.key() {
                            Key::Enter => { event.prevent_default(); if editing() { editing.set(false); let next=draft(); if next!=original() { oncommit.call(next); } } },
                            Key::Escape => { editing.set(false); },
                            _ => {}
                        }
                }
            } else {
                button { r#type: "button", class: if empty { "inline-field-value is-empty" } else { "inline-field-value" }, aria_label: "Edit {label}", onclick: {
                    let value = value.clone();
                    move |_| { draft.set(value.clone()); original.set(value.clone()); editing.set(true); }
                }, "{display}" }
            }
        }
    }
}
