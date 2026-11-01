use crate::{
    form_button::{ButtonVariant, FormButton},
    form_select::{FieldContext, FormSelect},
    icon::Icon,
    schedule_summary::display_time,
    shared_ui::Dialog,
};
use dioxus::prelude::*;

/// Shared time picker. Draft changes are committed only with Set time.
#[component]
pub fn TimePicker(
    label: &'static str,
    value: String,
    on_change: EventHandler<String>,
    #[props(default)] disabled: bool,
    #[props(default)] required: bool,
) -> Element {
    let field = try_use_context::<FieldContext>();
    let required = required || field.is_some_and(|field| field.required);
    let mut open = use_signal(|| false);
    let mut hour = use_signal(|| "9".to_string());
    let mut minute = use_signal(|| "00".to_string());
    let mut period = use_signal(|| "AM".to_string());
    let display = if value.is_empty() { "Choose a time".to_string() } else { display_time(&value) };
    let preview = format!("{}:{} {}", hour(), minute(), period());
    rsx! {
        div { class: "shared-time-picker",
            FormButton { class: if value.is_empty() { "date-picker-trigger time-picker-trigger empty" } else { "date-picker-trigger time-picker-trigger" },
                r#type: "button", disabled, aria_label: "{label}: {display}", aria_expanded: open(), aria_haspopup: "dialog",
                onclick: move |_| {
                    let parsed = chrono::NaiveTime::parse_from_str(&value, "%H:%M").ok();
                    use chrono::Timelike;
                    let h = parsed.map(|time| time.hour()).unwrap_or(9);
                    hour.set(((h + 11) % 12 + 1).to_string());
                    minute.set(format!("{:02}", parsed.map(|time| time.minute()).unwrap_or(0)));
                    period.set(if h >= 12 { "PM" } else { "AM" }.into());
                    open.set(true);
                },
                span { "{display}" } Icon { name: "clock" }
            }
            if open() {
                Dialog { label: format!("Choose {label}"), close_label: "Close time picker", card_class: "shared-time-picker-dialog",
                    on_close: move |_| open.set(false),
                    h2 { "{label}" }
                    div { class: "time-picker-preview", aria_live: "polite", "{preview}" }
                    div { class: "time-picker-fields",
                        FormSelect { label: "Hour", required: true, value: hour(), placeholder: "Hour",
                            options: (1..=12).map(|hour| (hour.to_string(), format!("{hour:02}"))).collect(), onchange: move |value| hour.set(value) }
                        FormSelect { label: "Minute", required: true, value: minute(), placeholder: "Minute",
                            options: (0..60).map(|minute| (format!("{minute:02}"), format!("{minute:02}"))).collect(), onchange: move |value| minute.set(value) }
                        FormSelect { label: "AM / PM", required: true, value: period(), placeholder: "AM / PM",
                            options: vec![("AM".into(), "AM".into()), ("PM".into(), "PM".into())], onchange: move |value| period.set(value) }
                    }
                    div { class: "shared-dialog-actions",
                        if !required { FormButton { variant: ButtonVariant::Text, r#type: "button", onclick: move |_| { on_change.call(String::new()); open.set(false); }, "Clear" } }
                        FormButton { variant: ButtonVariant::Outline, r#type: "button", onclick: move |_| open.set(false), "Cancel" }
                        FormButton { variant: ButtonVariant::Primary, r#type: "button", onclick: move |_| {
                            let h = hour().parse::<u32>().unwrap_or(9) % 12 + if period() == "PM" { 12 } else { 0 };
                            on_change.call(format!("{h:02}:{}", minute()));
                            open.set(false);
                        }, "Set time" }
                    }
                }
            }
        }
    }
}
