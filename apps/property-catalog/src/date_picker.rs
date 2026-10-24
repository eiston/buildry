use crate::icon::Icon;
use chrono::{Datelike, Duration, NaiveDate};
use dioxus::prelude::*;

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];

fn parse_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

fn today() -> NaiveDate {
    let now = js_sys::Date::new_0();
    NaiveDate::from_ymd_opt(
        now.get_full_year() as i32,
        now.get_month() + 1,
        now.get_date(),
    )
    .unwrap_or_else(|| NaiveDate::from_ymd_opt(2026, 1, 1).unwrap())
}

fn month_start(date: NaiveDate) -> NaiveDate {
    date.with_day(1).unwrap()
}

fn shift_month(date: NaiveDate, offset: i32) -> NaiveDate {
    let number = date.year() * 12 + date.month0() as i32 + offset;
    NaiveDate::from_ymd_opt(number.div_euclid(12), number.rem_euclid(12) as u32 + 1, 1)
        .unwrap_or(date)
}

#[component]
pub fn DatePicker(
    label: String,
    value: String,
    on_change: EventHandler<String>,
    #[props(default)] min: Option<String>,
    #[props(default)] max: Option<String>,
    #[props(default)] disabled: bool,
    #[props(default)] placeholder: Option<String>,
) -> Element {
    let selected = parse_date(&value);
    let current_day = today();
    let lower = min.as_deref().and_then(parse_date);
    let upper = max.as_deref().and_then(parse_date);
    let initial = selected.or(lower).unwrap_or(current_day);
    let mut visible_month = use_signal(|| month_start(initial));
    let mut open = use_signal(|| false);
    let month = visible_month();
    let month_label = format!("{} {}", MONTHS[month.month0() as usize], month.year());
    let first_weekday = month.weekday().num_days_from_sunday() as i64;
    let first_cell = month - Duration::days(first_weekday);
    let current_month = month.month();
    let previous_disabled = lower.is_some_and(|bound| shift_month(month, -1) < month_start(bound));
    let next_disabled = upper.is_some_and(|bound| shift_month(month, 1) > month_start(bound));
    let display = selected
        .map(|date| {
            format!(
                "{} {}, {}",
                MONTHS[date.month0() as usize],
                date.day(),
                date.year()
            )
        })
        .unwrap_or_else(|| placeholder.unwrap_or_else(|| "Choose a date".into()));

    rsx! {
        div { class: "date-picker",
            button {
                r#type: "button", class: if selected.is_some() { "date-picker-trigger" } else { "date-picker-trigger empty" },
                disabled, aria_expanded: open(), aria_label: "{label}: {display}",
                onclick: move |_| { if !open() { visible_month.set(month_start(selected.or(lower).unwrap_or(current_day))); } open.toggle(); },
                span { "{display}" } span { class: "date-picker-icon", aria_hidden: "true", Icon { name: "calendar" } }
            }
            if open() {
                button { r#type: "button", class: "date-picker-backdrop", aria_label: "Close date picker", onclick: move |_| open.set(false) }
                div { class: "date-picker-panel", role: "dialog", aria_label: "Choose {label}",
                    div { class: "date-picker-heading",
                        button { r#type: "button", aria_label: "Previous month", disabled: previous_disabled, onclick: move |_| visible_month.set(shift_month(visible_month(), -1)), Icon { name: "chevron-left" } }
                        strong { "{month_label}" }
                        button { r#type: "button", aria_label: "Next month", disabled: next_disabled, onclick: move |_| visible_month.set(shift_month(visible_month(), 1)), Icon { name: "chevron-right" } }
                    }
                    div { class: "date-picker-jump",
                        crate::form_select::FormSelect { label: "Month", required: true, value: month.month().to_string(), placeholder: "Month", options: MONTHS.iter().enumerate().map(|(index,name)|((index+1).to_string(),(*name).into())).collect::<Vec<_>>(), onchange: move |value:String| {
                            if let Ok(number) = value.parse::<u32>() { if let Some(date) = NaiveDate::from_ymd_opt(visible_month().year(), number, 1) { visible_month.set(date); } }
                        } }
                        crate::form_select::FormSelect { label: "Year", required: true, value: month.year().to_string(), placeholder: "Year", options: (1900..=2100).map(|year|(year.to_string(),year.to_string())).collect::<Vec<_>>(), onchange: move |value:String| {
                            if let Ok(year) = value.parse::<i32>() { if let Some(date) = NaiveDate::from_ymd_opt(year, visible_month().month(), 1) { visible_month.set(date); } }
                        } }
                    }
                    div { class: "date-picker-grid",
                        for name in WEEKDAYS { span { class: "date-picker-weekday", "{name}" } }
                        for offset in 0..42 {
                            { let date = first_cell + Duration::days(offset);
                              let out_of_range = lower.is_some_and(|bound| date < bound) || upper.is_some_and(|bound| date > bound);
                              let date_value = date.format("%Y-%m-%d").to_string();
                              let date_label = date.format("%B %e, %Y").to_string();
                              rsx! { button {
                                  key: "{date_value}", r#type: "button",
                                  class: if selected == Some(date) { "date-picker-day selected" } else if date.month() != current_month { "date-picker-day outside" } else if date == current_day { "date-picker-day today" } else { "date-picker-day" },
                                  disabled: out_of_range, aria_label: "{date_label}", aria_pressed: selected == Some(date),
                                  onclick: move |_| { on_change.call(date_value.clone()); open.set(false); }, "{date.day()}"
                              } }
                            }
                        }
                    }
                    div { class: "date-picker-footer",
                        button { r#type: "button", disabled: lower.is_some_and(|bound| current_day < bound) || upper.is_some_and(|bound| current_day > bound), onclick: move |_| { on_change.call(current_day.format("%Y-%m-%d").to_string()); open.set(false); }, "Today" }
                        if selected.is_some() { button { r#type: "button", onclick: move |_| { on_change.call(String::new()); open.set(false); }, "Clear" } }
                    }
                }
            }
        }
    }
}
