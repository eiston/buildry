use crate::{
    components::forms::form_button::{ButtonVariant, FormButton},
    components::forms::form_select::FieldContext,
    components::icon::Icon,
    components::schedule_summary::display_time,
    components::shared_ui::Dialog,
};
use dioxus::prelude::*;

/// Analog time selection with a draft that is applied only with Set time.
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
    let mut hour = use_signal(|| 9_u32);
    let mut minute = use_signal(|| 0_u32);
    let mut afternoon = use_signal(|| false);
    let mut choosing_hour = use_signal(|| true);
    let display = if value.is_empty() { "Choose a time".to_string() } else { display_time(&value) };
    let angle = if choosing_hour() { f64::from(hour() % 12) * 30.0 } else { f64::from(minute()) * 6.0 };
    rsx! {
        div { class: "shared-time-picker",
            FormButton { class: if value.is_empty() { "date-picker-trigger time-picker-trigger empty" } else { "date-picker-trigger time-picker-trigger" },
                r#type: "button", disabled, aria_label: "{label}: {display}", aria_expanded: open(), aria_haspopup: "dialog",
                onclick: move |_| {
                    use chrono::Timelike;
                    let parsed = chrono::NaiveTime::parse_from_str(&value, "%H:%M").ok();
                    let h = parsed.map(|time| time.hour()).unwrap_or(9);
                    hour.set((h + 11) % 12 + 1);
                    minute.set(parsed.map(|time| time.minute()).unwrap_or(0));
                    afternoon.set(h >= 12);
                    choosing_hour.set(true);
                    open.set(true);
                },
                span { "{display}" } Icon { name: "clock" }
            }
            if open() {
                Dialog { label: format!("Choose {label}"), close_label: "Close time picker", card_class: "shared-time-picker-dialog",
                    on_close: move |_| open.set(false),
                    h2 { "{label}" }
                    div { class: "clock-time-display",
                        div { class: "clock-digits",
                            FormButton { r#type: "button", class: if choosing_hour() { "clock-digit selected" } else { "clock-digit" }, aria_label: "Choose hour", aria_pressed: choosing_hour(), onclick: move |_| choosing_hour.set(true), "{hour():02}" }
                            span { class: "clock-colon", aria_hidden: "true", ":" }
                            FormButton { r#type: "button", class: if !choosing_hour() { "clock-digit selected" } else { "clock-digit" }, aria_label: "Choose minute", aria_pressed: !choosing_hour(), onclick: move |_| choosing_hour.set(false), "{minute():02}" }
                        }
                        div { class: "clock-period", role: "group", aria_label: "AM or PM",
                            FormButton { r#type: "button", class: if !afternoon() { "selected" } else { "" }, aria_pressed: !afternoon(), onclick: move |_| afternoon.set(false), "AM" }
                            FormButton { r#type: "button", class: if afternoon() { "selected" } else { "" }, aria_pressed: afternoon(), onclick: move |_| afternoon.set(true), "PM" }
                        }
                    }
                    div { class: "clock-face", role: "group", aria_label: if choosing_hour() { "Hours" } else { "Minutes" },
                        onkeydown: move |event: KeyboardEvent| {
                            let delta = match event.key() { Key::ArrowUp | Key::ArrowRight => 1, Key::ArrowDown | Key::ArrowLeft => -1, _ => return };
                            event.prevent_default();
                            if choosing_hour() { hour.set((hour() as i32 - 1 + delta).rem_euclid(12) as u32 + 1); }
                            else { minute.set((minute() as i32 + delta).rem_euclid(60) as u32); }
                        },
                        span { class: if choosing_hour() { "clock-hand" } else { "clock-hand minute-hand" }, aria_hidden: "true", style: "transform:translateX(-50%) rotate({angle}deg)" }
                        span { class: "clock-center", aria_hidden: "true" }
                        for index in 0..12_u32 {
                            {
                                let radians = f64::from(index) * std::f64::consts::PI / 6.0;
                                let left = 50.0 + 40.0 * radians.sin();
                                let top = 50.0 - 40.0 * radians.cos();
                                let number = if choosing_hour() { if index == 0 { 12 } else { index } } else { index * 5 };
                                let selected = if choosing_hour() { hour() == number } else { minute() == number };
                                let text = if choosing_hour() { number.to_string() } else { format!("{number:02}") };
                                rsx! {
                                    FormButton { key: "{index}", r#type: "button", class: if selected { "clock-number selected" } else { "clock-number" },
                                        style: "left:{left}%;top:{top}%", aria_label: if choosing_hour() { format!("Hour {number}") } else { format!("Minute {number:02}") }, aria_pressed: selected,
                                        onclick: move |_| { if choosing_hour() { hour.set(number); choosing_hour.set(false); } else { minute.set(number); } }, "{text}"
                                    }
                                }
                            }
                        }
                    }
                    div { class: if choosing_hour() { "clock-precision inactive" } else { "clock-precision" }, aria_hidden: choosing_hour(),
                        FormButton { variant: ButtonVariant::Icon, r#type: "button", disabled: choosing_hour(), aria_label: "Previous minute", onclick: move |_| minute.set((minute() + 59) % 60), Icon { name: "minus" } }
                        FormButton { variant: ButtonVariant::Icon, r#type: "button", disabled: choosing_hour(), aria_label: "Next minute", onclick: move |_| minute.set((minute() + 1) % 60), Icon { name: "plus" } }
                    }
                    div { class: "shared-dialog-actions",
                        if !required { FormButton { variant: ButtonVariant::Text, r#type: "button", onclick: move |_| { on_change.call(String::new()); open.set(false); }, "Clear" } }
                        FormButton { variant: ButtonVariant::Text, r#type: "button", onclick: move |_| open.set(false), "Cancel" }
                        FormButton { variant: ButtonVariant::Primary, r#type: "button", onclick: move |_| {
                            let h = hour() % 12 + if afternoon() { 12 } else { 0 };
                            on_change.call(format!("{h:02}:{:02}", minute()));
                            open.set(false);
                        }, "Set time" }
                    }
                }
            }
        }
    }
}
