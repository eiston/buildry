use dioxus::prelude::*;

/// Shared disclosure shell used by cards with either standard or custom headings.
#[component]
pub fn CollapsibleCard(
    summary: Element,
    children: Element,
    #[props(default)] class: Option<String>,
    #[props(default)] summary_class: Option<String>,
    #[props(default)] content_class: Option<String>,
    #[props(default)] initially_open: bool,
) -> Element {
    rsx! {
        details { class: format!("collapsible-card {}", class.unwrap_or_default()), open: initially_open,
            summary { class: summary_class, {summary} }
            if let Some(content_class) = content_class { div { class: content_class, {children} } }
            else { {children} }
        }
    }
}

#[component]
pub fn CollapsiblePanel(
    title: String,
    subtitle: Option<String>,
    badge: Option<String>,
    class: String,
    initially_open: bool,
    children: Element,
) -> Element {
    rsx! {
        CollapsibleCard { class: "collapsible-panel {class}", summary_class: None, content_class: Some("collapsible-panel-content".into()), initially_open,
            summary: rsx! {
                span { class: "collapsible-panel-summary",
                    strong { "{title}" }
                    if let Some(subtitle) = subtitle { small { "{subtitle}" } }
                }
                if let Some(badge) = badge { span { class: "collapsible-panel-badge", "{badge}" } }
                span { class: "collapsible-panel-chevron", aria_hidden: "true", crate::icon::Icon { name: "chevron-down" } }
            },
            {children}
        }
    }
}

/// The property details card is the standard shell for editable forms.
#[component]
pub fn FormCard(
    title: String,
    children: Element,
    #[props(default)] class: Option<String>,
    #[props(default = true)] initially_open: bool,
) -> Element {
    rsx! {
        CollapsiblePanel {
            title,
            subtitle: None,
            badge: None,
            class: format!("shared-form-card {}", class.unwrap_or_default()),
            initially_open,
            {children}
        }
    }
}
