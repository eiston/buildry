use crate::{form_button::{ButtonVariant, FormButton}, icon::Icon};
use dioxus::prelude::*;

/// Modal structure; focus, Escape, and restoration are handled by form-interactions.
#[component]
pub fn Dialog(
    label: String,
    on_close: EventHandler<()>,
    children: Element,
    #[props(default)] backdrop_class: String,
    #[props(default)] card_class: String,
    #[props(default)] close_label: Option<String>,
    #[props(default)] busy: bool,
    #[props(default)] alert: bool,
    #[props(default)] swipe_to_close: bool,
    #[props(default = true)] show_close: bool,
) -> Element {
    let close_label = close_label.unwrap_or_else(|| format!("Close {label}"));
    rsx! {
        document::Script { src: "/assets/dialog-interactions.mjs", r#type: "module" }
        div { class: "shared-dialog-backdrop {backdrop_class}", role: if alert { "alertdialog" } else { "dialog" }, aria_modal: "true", aria_label: label,
            div { class: "shared-dialog-card {card_class}", "data-dialog-sheet": swipe_to_close.to_string(),
                if show_close {
                    FormButton { r#type: "button", variant: ButtonVariant::Icon, class: "shared-dialog-close", aria_label: close_label, disabled: busy,
                        onclick: move |_| on_close.call(()), Icon { name: "close" }
                    }
                }
                {children}
            }
        }
    }
}

#[component]
pub fn ConfirmDialog(
    title: String,
    confirm_label: String,
    on_confirm: EventHandler<()>,
    on_close: EventHandler<()>,
    children: Element,
    #[props(default)] busy: bool,
    #[props(default = "Cancel".into())] cancel_label: String,
) -> Element {
    rsx! {
        Dialog { label: title.clone(), close_label: "Close confirmation", on_close, busy, alert: true, card_class: "shared-confirm-dialog",
            h2 { "{title}" }
            {children}
            div { class: "shared-dialog-actions",
                FormButton { r#type: "button", variant: ButtonVariant::Outline, disabled: busy, onclick: move |_| on_close.call(()), "{cancel_label}" }
                FormButton { r#type: "button", variant: ButtonVariant::Danger, busy, onclick: move |_| on_confirm.call(()), "{confirm_label}" }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum PageStateKind { Loading, #[default] Empty, Error, Missing }

/// Concise state content. Recovery actions are supplied by the owning page.
#[component]
pub fn PageState(
    title: String,
    #[props(default)] kind: PageStateKind,
    #[props(default)] page: bool,
    #[props(default)] class: String,
    children: Element,
) -> Element {
    let role = match kind { PageStateKind::Loading => Some("status"), PageStateKind::Error => Some("alert"), _ => None };
    rsx! { div { class: "shared-page-state {class}", role,
        if kind == PageStateKind::Loading || kind == PageStateKind::Error { p { "{title}" } }
        else if page { h1 { "{title}" } }
        else { h2 { "{title}" } }
        {children}
    } }
}

/// Layout for existing shared filter controls, with an optional clear action.
#[component]
pub fn FilterBar(
    children: Element,
    #[props(default)] class: String,
    #[props(default = "Filter results".into())] label: String,
    #[props(default)] active: bool,
    #[props(default)] on_clear: Option<EventHandler<()>>,
) -> Element {
    rsx! { div { class: "shared-filter-bar {class}", aria_label: label,
        if active { if let Some(on_clear) = on_clear {
            FormButton { r#type: "button", variant: ButtonVariant::Text, class: "filter-bar-clear", onclick: move |_| on_clear.call(()), "Clear" }
        } }
        {children}
    } }
}
