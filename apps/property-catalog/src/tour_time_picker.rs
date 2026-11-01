use crate::{date_picker::DatePicker, form_button::FormButton, tour_model::{TOUR_SLOTS, window_label}};
use dioxus::prelude::*;

/// Availability stays in the owning page; host and guest use the same controls.
#[component]
pub fn TourTimePicker(
    date: String,
    slot: String,
    available: Vec<String>,
    on_date: EventHandler<String>,
    on_slot: EventHandler<String>,
    #[props(default)] min: Option<String>,
    #[props(default)] max: Option<String>,
    #[props(default)] loading: bool,
    #[props(default)] disabled: bool,
    #[props(default)] error: String,
) -> Element {
    rsx! {
        div { class: "tour-reschedule-date date-field", span { "New date" }
            DatePicker { label: "New tour date", value: date, min, max, disabled, on_change: on_date }
        }
        span { class: "tour-detail-label", "START TIME · TWO-HOUR WINDOW" }
        if loading { p { class: "tour-reschedule-hint", role: "status", "Checking available times…" } }
        else if !error.is_empty() { p { class: "booking-error", role: "alert", "{error}" } }
        else if available.is_empty() { p { class: "tour-reschedule-hint", role: "status", "No times available for this date." } }
        else {
            div { class: "tour-reschedule-times", role: "group", aria_label: "Available start times",
                for (key, label) in TOUR_SLOTS {
                    if available.iter().any(|slot| slot == key) {
                        FormButton { r#type: "button", key: "{key}", class: if slot == key { "tour-time-choice selected" } else { "tour-time-choice" },
                            disabled, aria_pressed: slot == key, onclick: move |_| on_slot.call(key.into()), "{label}"
                        }
                    }
                }
            }
        }
        if !slot.is_empty() && error.is_empty() && !loading { p { class: "tour-reschedule-hint", "{window_label(&slot)} · Toronto time" } }
    }
}
