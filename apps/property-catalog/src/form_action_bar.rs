use crate::icon::Icon;
use dioxus::prelude::*;

#[component]
pub fn FormActionBar(
    back_href: String,
    back_label: String,
    #[props(default)] back_step: Option<Signal<u8>>,
    #[props(default)] title: Option<String>,
    #[props(default)] on_back: Option<EventHandler<()>>,
    #[props(default)] center: Option<Element>,
    #[props(default)] middle: Option<Element>,
    #[props(default)] class_name: Option<String>,
    children: Element,
) -> Element {
    let class_name = class_name.unwrap_or_default();
    rsx! {
        div { class: "form-action-bar {class_name}",
            if let Some(on_back) = on_back {
                button { class: "back-icon-button", r#type: "button", aria_label: "{back_label}", title: "{back_label}", onclick: move |_| on_back.call(()), Icon { name: "arrow-left" } }
            } else if let Some(mut step) = back_step {
                button { class: "back-icon-button", r#type: "button", aria_label: "Previous step", title: "Previous step", onclick: move |_| {
                    step.set(step().saturating_sub(1).max(1));
                    if let Some(window) = web_sys::window() { window.scroll_to_with_x_and_y(0.0, 0.0); }
                }, Icon { name: "arrow-left" } }
            } else {
                a { class: "back-icon-button", href: "{back_href}", aria_label: "{back_label}", title: "{back_label}", Icon { name: "arrow-left" } }
            }
            if let Some(title)=title {span {class:"form-action-title","{title}"}}
            if let Some(middle)=middle { div { class: "form-action-bar-middle", {middle} } }
            if let Some(center)=center { div { class: "form-action-bar-center", {center} } }
            div { class: "form-action-bar-actions", {children} }
        }
    }
}

#[component]
pub fn FormWizardHeader(
    eyebrow: String,
    title: String,
    description: String,
    step: u8,
    total: u8,
) -> Element {
    let progress = if total == 0 {
        0
    } else {
        u32::from(step) * 100 / u32::from(total)
    };
    rsx! {
        div { class: "form-wizard-header",
            div { class: "lease-wizard-progress", aria_label: "{title} progress",
                span { "STEP {step} OF {total}" }
                div { class: "lease-wizard-progress-track", span { style: "width: {progress}%" } }
            }
            div { class: "lease-wizard-heading",
                span { "{eyebrow}" }
                h1 { "{title}" }
                if !description.is_empty() { p { "{description}" } }
            }
        }
    }
}

#[component]
pub fn PropertyChoice(
    image: String,
    name: String,
    subtitle: String,
    selected: bool,
    on_select: EventHandler<()>,
) -> Element {
    rsx! {
        button { r#type: "button", class: if selected {"lease-wizard-option selected"} else {"lease-wizard-option"}, aria_pressed: selected, onclick: move |_| on_select.call(()),
            img { src: "{image}", alt: "" }
            span { strong { "{name}" } small { "{subtitle}" } }
            span { class: "lease-wizard-option-check", Icon { name: "check" } }
        }
    }
}

#[component]
pub fn PropertySummary(image: String, name: String, subtitle: String) -> Element {
    rsx! { div { class: "property-summary",
        img { src: "{image}", alt: "" }
        div { span { "PROPERTY" } strong { "{name}" } small { "{subtitle}" } }
    } }
}

/// Shared property and guest context for tour and lease workflows.
#[component]
pub fn TourPartySummary(
    image: String,
    property_name: String,
    property_subtitle: String,
    guest_name: String,
    guest_email: String,
    guest_phone: String,
) -> Element {
    rsx! {
        section { class: "tour-party-summary", aria_label: "Property and guest",
            PropertySummary { image, name: property_name, subtitle: property_subtitle }
            div { class: "guest-contact-summary",
                span { class: "tour-detail-label", "GUEST" }
                div { class: "guest-contact-summary-row",
                    div { class: "guest-contact-summary-info",
                        strong { "{guest_name}" }
                        if !guest_email.is_empty() { a { href: "mailto:{guest_email}", "{guest_email}" } }
                    }
                    if !guest_phone.is_empty() {
                        a { href: "tel:{guest_phone}", class: "guest-contact-phone glass-icon-button", aria_label: "Call {guest_phone}", title: "Call {guest_phone}", Icon { name: "phone" } }
                    }
                }
            }
        }
    }
}

#[component]
pub fn StaySummary(start: String, months: u32) -> Element {
    let end = chrono::NaiveDate::parse_from_str(&start, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.checked_add_months(chrono::Months::new(months)))
        .and_then(|date| date.pred_opt())
        .map(|date| date.format("%b %-d, %Y").to_string())
        .unwrap_or_else(|| "End date pending".into());
    let display_start = chrono::NaiveDate::parse_from_str(&start, "%Y-%m-%d")
        .map(|date| date.format("%b %-d, %Y").to_string())
        .unwrap_or(start);
    let length = format!(
        "{} {}",
        months,
        if months == 1 { "month" } else { "months" }
    );
    rsx! { div { class:"stay-summary",
        div { span { "MOVE-IN" } strong { "{display_start}" } }
        div { class:"stay-summary-connector", Icon { name:"arrow-right" } }
        div { span { "MOVE-OUT" } strong { "{end}" } }
        small { "{length}" }
    } }
}
