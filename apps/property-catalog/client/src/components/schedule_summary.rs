use chrono::NaiveDate;
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Default)]
pub enum ScheduleLayout { #[default] Inline, Stacked }

pub fn display_date(date: &str) -> String {
    NaiveDate::parse_from_str(date, "%Y-%m-%d").map(|date| date.format("%a, %b %-d, %Y").to_string()).unwrap_or_else(|_| date.into())
}
pub fn display_time(time: &str) -> String {
    chrono::NaiveTime::parse_from_str(time, "%H:%M").map(|time| time.format("%-I:%M %p").to_string()).unwrap_or_else(|_| time.into())
}

#[component]
pub fn ScheduleSummary(
    date: String,
    #[props(default)] time: String,
    #[props(default)] end_time: Option<String>,
    #[props(default)] duration_minutes: Option<i32>,
    #[props(default)] timezone: Option<String>,
    #[props(default)] layout: ScheduleLayout,
    #[props(default)] class: String,
) -> Element {
    let time = display_time(&time);
    let time = end_time.map(|end| format!("{time}–{}", display_time(&end))).unwrap_or(time);
    rsx! { span { class: format!("shared-schedule-summary {} {class}", if layout == ScheduleLayout::Stacked { "stacked" } else { "inline" }),
        time { datetime: date.clone(), "{display_date(&date)}" }
        if !time.is_empty() { span { "{time}" } }
        if let Some(minutes) = duration_minutes { span { "{minutes} min" } }
        if let Some(timezone) = timezone { span { "{timezone}" } }
    } }
}
