use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Default)]
pub enum ButtonVariant { #[default] Primary, Outline, Danger, Icon, Text }

impl ButtonVariant {
    fn class(self) -> &'static str {
        match self { Self::Primary => "primary-button", Self::Outline => "outline-button", Self::Danger => "danger-button", Self::Icon => "icon-button", Self::Text => "text-button" }
    }
}

/// Shared action control; callers keep their labels and native submit behavior.
#[component]
pub fn FormButton(
    children: Element,
    #[props(default)] variant: Option<ButtonVariant>,
    #[props(default)] class: Option<String>,
    #[props(default)] r#type: Option<String>,
    #[props(default)] disabled: bool,
    #[props(default)] busy: bool,
    #[props(default)] aria_label: Option<String>,
    #[props(default)] title: Option<String>,
    #[props(default)] form: Option<String>,
    #[props(default)] id: Option<String>,
    #[props(default)] style: Option<String>,
    #[props(default)] aria_pressed: Option<bool>,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
) -> Element {
    rsx! { button {
        class: format!("shared-form-button {} {}", variant.map(ButtonVariant::class).unwrap_or_else(|| if class.is_none() { "primary-button" } else { "" }), class.unwrap_or_default()),
        form, id, style, aria_pressed, r#type, disabled: disabled || busy, aria_busy: busy, aria_label, title,
        onclick: move |event| { if let Some(handler) = onclick { handler.call(event); } },
        {children}
    } }
}
