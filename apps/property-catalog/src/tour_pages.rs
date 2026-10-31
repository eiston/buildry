use crate::collapsible_panel::CollapsiblePanel;
use crate::icon::Icon;
use crate::{
    date_picker::DatePicker,
    guest_model::{rental_period_label, term_label},
    housework_model::HouseworkTicket,
    tour_api,
    tour_model::{
        slot_label, window_label, GuestBooking, GuestBookingStatus, NewTour, RescheduleTour,
        TourBooking, TourHouse, TourOptions, TOUR_SLOTS,
    },
};
use chrono::{Datelike, Duration, NaiveDate};
use dioxus::prelude::*;
use gloo_net::http::Request;
use gloo_timers::future::TimeoutFuture;
use wasm_bindgen_futures::JsFuture;

struct CalendarDay {
    date: String,
    weekday: String,
    day_number: String,
    is_today: bool,
    events: Vec<CalendarEvent>,
}

struct CalendarEvent {
    booking: TourBooking,
    style: String,
}

#[derive(Clone, Copy)]
pub struct CalendarUiState {
    pub today: Signal<String>,
    pub week_offset: Signal<i64>,
    pub selected_day: Signal<String>,
    pub start_offset: Signal<i64>,
    pub end_offset: Signal<i64>,
    pub compact: Signal<bool>,
    pub search_open: Signal<bool>,
    pub search: Signal<String>,
}

pub fn use_calendar_ui_state() -> CalendarUiState {
    CalendarUiState {
        today: use_signal(String::new),
        week_offset: use_signal(|| 0),
        selected_day: use_signal(String::new),
        start_offset: use_signal(|| -14),
        end_offset: use_signal(|| 42),
        compact: use_signal(|| false),
        search_open: use_signal(|| false),
        search: use_signal(String::new),
    }
}

fn admin_house_label(houses: &[TourHouse], id: u32, name: &str) -> String {
    match houses.iter().find(|house| house.id == id) {
        Some(house) if !house.address.is_empty() => {
            format!("House {id} · {name} · {}", house.address)
        }
        _ => format!("House {id} · {name} · Address not set"),
    }
}

async fn copy_link(url: String) -> Result<(), String> {
    let window = web_sys::window().ok_or("Clipboard is unavailable.")?;
    JsFuture::from(window.navigator().clipboard().write_text(&url))
        .await
        .map_err(|_| "Could not copy the link. Select and copy it instead.".to_string())?;
    Ok(())
}

fn manage_url(token: &str) -> String {
    web_sys::window()
        .and_then(|window| window.location().origin().ok())
        .map(|origin| format!("{origin}/booking/{token}"))
        .unwrap_or_else(|| format!("/booking/{token}"))
}

fn booking_return_path() -> String {
    let query = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .unwrap_or_default();
    let return_to = query.trim_start_matches('?').split('&').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        if key == "return_to" {
            urlencoding::decode(value)
                .ok()
                .map(|value| value.into_owned())
        } else {
            None
        }
    });
    if let Some(path) = return_to {
        let pathname = path.split('?').next().unwrap_or_default();
        let segments: Vec<&str> = pathname.trim_matches('/').split('/').collect();
        let valid = pathname == "/stays"
            || (segments.len() == 3
                && segments[0] == "stays"
                && segments[1] == "house"
                && segments[2].parse::<u32>().ok().is_some_and(|id| id > 0))
            || (segments.len() == 4
                && segments[0] == "stays"
                && segments[1] == "room"
                && segments[2].parse::<u32>().ok().is_some_and(|id| id > 0)
                && segments[3].parse::<u32>().is_ok());
        let safe_query = path
            .split_once('?')
            .map(|(_, query)| {
                let pairs: Vec<&str> = query.split('&').collect();
                pairs.len() == 3
                    && pairs[0]
                        .strip_prefix("year=")
                        .and_then(|value| value.parse::<i32>().ok())
                        .is_some_and(|year| (2020..=2100).contains(&year))
                    && pairs[1]
                        .strip_prefix("start=")
                        .is_some_and(|value| ["sep", "jan", "may"].contains(&value))
                    && pairs[2]
                        .strip_prefix("months=")
                        .and_then(|value| value.parse::<u8>().ok())
                        .is_some_and(|months| [4, 8, 12, 24, 36].contains(&months))
            })
            .unwrap_or(true);
        if valid && safe_query && path.starts_with('/') && !path.contains('#') {
            return path;
        }
    }
    "/stays".into()
}

fn layout_events(bookings: Vec<TourBooking>) -> Vec<CalendarEvent> {
    let mut lane_ends: Vec<i32> = Vec::new();
    let mut assigned = Vec::new();
    for booking in bookings {
        let hour = booking
            .slot_start
            .split(':')
            .next()
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or(10);
        let lane = if let Some(index) = lane_ends.iter().position(|end| *end <= hour) {
            lane_ends[index] = hour + 2;
            index
        } else {
            lane_ends.push(hour + 2);
            lane_ends.len() - 1
        };
        assigned.push((booking, hour, lane));
    }
    let lane_count = lane_ends.len().max(1);
    assigned
        .into_iter()
        .map(|(booking, hour, lane)| CalendarEvent {
            booking,
            style: format!(
                "top:{}px;left:calc({:.3}% + 4px);width:calc({:.3}% - 8px);height:132px",
                (hour - 10) * 70 + 4,
                lane as f64 * 100.0 / lane_count as f64,
                100.0 / lane_count as f64,
            ),
        })
        .collect()
}

fn display_tour_date(date: &str) -> String {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|value| value.format("%A, %B %-d, %Y").to_string())
        .unwrap_or_else(|_| date.to_string())
}

fn agenda_date(date: &str) -> String {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|value| value.format("%b %-d").to_string())
        .unwrap_or_else(|_| date.to_string())
}

fn scroll_to_agenda_date(date: String) {
    spawn(async move {
        for _ in 0..20 {
            TimeoutFuture::new(50).await;
            let Some(document) = web_sys::window().and_then(|window| window.document()) else {
                return;
            };
            let Some(list) = document.get_element_by_id("tour-agenda-list") else {
                continue;
            };
            let Some(day) = document.get_element_by_id(&format!("agenda-{date}")) else {
                continue;
            };
            let list_top = list.get_bounding_client_rect().top();
            let header_bottom = document
                .query_selector(".calendar-topbar")
                .ok()
                .flatten()
                .map(|header| header.get_bounding_client_rect().bottom())
                .unwrap_or(list_top);
            let distance = day.get_bounding_client_rect().top() - header_bottom.max(list_top) - 8.0;
            list.set_scroll_top(list.scroll_top() + distance.round() as i32);
            return;
        }
    });
}

fn scroll_to_agenda_week_date(date: String) {
    spawn(async move {
        TimeoutFuture::new(35).await;
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let Some(week) = document.get_element_by_id("tour-agenda-week") else {
            return;
        };
        let Ok(selected) = NaiveDate::parse_from_str(&date, "%Y-%m-%d") else {
            return;
        };
        let sunday = selected - Duration::days(selected.weekday().num_days_from_sunday() as i64);
        let Some(day) =
            document.get_element_by_id(&format!("agenda-week-{}", sunday.format("%Y-%m-%d")))
        else {
            return;
        };
        let distance =
            day.get_bounding_client_rect().left() - week.get_bounding_client_rect().left() - 6.0;
        week.set_scroll_left(week.scroll_left() + distance.round() as i32);
    });
}

fn ripple_auto_selected_agenda_date(date: String) {
    spawn(async move {
        TimeoutFuture::new(35).await;
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let Some(day) = document.get_element_by_id(&format!("agenda-week-{date}")) else {
            return;
        };
        if !day
            .get_attribute("class")
            .is_some_and(|class| class.split_whitespace().any(|name| name == "selected"))
        {
            return;
        }
        if let Ok(event) = web_sys::Event::new("calendar-date-auto-selected") {
            let _ = day.dispatch_event(&event);
        }
    });
}

fn jump_to_agenda_date(
    date: String,
    today: &str,
    mut selected_day: Signal<String>,
    mut start_offset: Signal<i64>,
    mut end_offset: Signal<i64>,
    mut week_offset: Signal<i64>,
) {
    let Ok(day) = NaiveDate::parse_from_str(&date, "%Y-%m-%d") else {
        return;
    };
    let Ok(current) = NaiveDate::parse_from_str(today, "%Y-%m-%d") else {
        return;
    };
    let delta = (day - current).num_days();
    if delta < start_offset() {
        start_offset.set(delta - 7);
    }
    if delta > end_offset() {
        end_offset.set(delta + 30);
    }
    let current_sunday = current - Duration::days(current.weekday().num_days_from_sunday() as i64);
    let target_sunday = day - Duration::days(day.weekday().num_days_from_sunday() as i64);
    week_offset.set((target_sunday - current_sunday).num_days() / 7);
    selected_day.set(date.clone());
    scroll_to_agenda_date(date);
}

#[component]
pub fn CalendarTopBar() -> Element {
    let CalendarUiState {
        today,
        week_offset,
        selected_day: selected_agenda_day,
        start_offset: agenda_start_offset,
        end_offset: agenda_end_offset,
        compact: agenda_compact,
        search_open: mut calendar_search_open,
        search: mut calendar_search,
    } = use_context::<CalendarUiState>();
    let selected_date = if selected_agenda_day().is_empty() {
        today()
    } else {
        selected_agenda_day()
    };
    let month = NaiveDate::parse_from_str(&selected_date, "%Y-%m-%d")
        .map(|date| date.format("%B").to_string())
        .unwrap_or_else(|_| "Calendar".into());
    let mut week_days = Vec::new();
    if let Ok(current) = NaiveDate::parse_from_str(&today(), "%Y-%m-%d") {
        let first = current + Duration::days(agenda_start_offset());
        let last = current + Duration::days(agenda_end_offset());
        let first_sunday = first - Duration::days(first.weekday().num_days_from_sunday() as i64);
        let last_saturday = last + Duration::days(6 - last.weekday().num_days_from_sunday() as i64);
        for offset in 0..=(last_saturday - first_sunday).num_days() {
            let date = first_sunday + Duration::days(offset);
            week_days.push(CalendarDay {
                date: date.format("%Y-%m-%d").to_string(),
                weekday: date.format("%a").to_string(),
                day_number: date.format("%-d").to_string(),
                is_today: date == current,
                events: Vec::new(),
            });
        }
    }
    rsx! {
        div { class: if agenda_compact() { "tour-agenda-sticky compact" } else { "tour-agenda-sticky" },
            if calendar_search_open() {
                div { class: "tour-agenda-search-bar",
                    button { r#type: "button", class: "back-icon-button", aria_label: "Close calendar search", onclick: move |_| { calendar_search_open.set(false); calendar_search.set(String::new()); scroll_to_agenda_date(selected_agenda_day()); }, Icon { name: "arrow-left" } }
                    crate::form_input::FormInput { r#type: "search", aria_label: "Search tours", placeholder: "Search guests, properties or dates", autofocus: true, value: "{calendar_search}", oninput: move |event: FormEvent| calendar_search.set(event.value()) }
                }
            } else {
                div { class: "tour-agenda-head",
                    h2 { "{month}" }
                    button { r#type: "button", class: "header-search-action", aria_label: "Search tours", title: "Search tours", onclick: move |_| calendar_search_open.set(true), Icon { name: "search" } }
                }
                div { id: "tour-agenda-week", class: "tour-agenda-week", aria_label: "Calendar dates",
                    for day in &week_days {
                        button { key: "{day.date}", id: "agenda-week-{day.date}", r#type: "button", class: format!("{}{}", if selected_agenda_day() == day.date && day.is_today { "selected today" } else if selected_agenda_day() == day.date { "selected" } else if day.is_today { "today" } else { "" }, if day.weekday == "Sun" { " week-start" } else { "" }), aria_label: "{day.weekday} {day.date}", onclick: { let date = day.date.clone(); move |_| jump_to_agenda_date(date.clone(), &today(), selected_agenda_day, agenda_start_offset, agenda_end_offset, week_offset) }, span { "{day.weekday}" } strong { "{day.day_number}" } }
                    }
                }
            }
        }
    }
}

fn display_rental_period(start: &str, months: u8, year: Option<i32>) -> String {
    if let Ok(date) = NaiveDate::parse_from_str(start, "%Y-%m-%d") {
        let season = match date.month() {
            1 => "jan",
            5 => "may",
            9 => "sep",
            _ => return format!("{start} · {months} months"),
        };
        return rental_period_label(date.year(), season, months);
    }
    year.map(|year| rental_period_label(year, start, months))
        .unwrap_or_else(|| term_label(start, months).unwrap_or("Selected term").into())
}

pub fn guest_page() -> Element {
    let return_path = booking_return_path();
    let (rental_year, rental_season, rental_months) = crate::guest_pages::default_period();
    let rental_start = format!(
        "{rental_year}-{}-01",
        match rental_season.as_str() {
            "jan" => "01",
            "may" => "05",
            _ => "09",
        }
    );
    let rental_label = rental_period_label(rental_year, &rental_season, rental_months);
    let locked_house_id = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .and_then(|query| {
            query.trim_start_matches('?').split('&').find_map(|part| {
                let (key, value) = part.split_once('=')?;
                if key == "property_id" {
                    value.parse::<u32>().ok().filter(|id| *id > 0)
                } else {
                    None
                }
            })
        });
    let mut options = use_signal(|| None::<TourOptions>);
    let mut listings = use_signal(Vec::<crate::guest_model::GuestListing>::new);
    let mut selected_house = use_signal(|| locked_house_id.unwrap_or(0));
    let mut tour_date = use_signal(String::new);
    let mut selected_slot = use_signal(String::new);
    let mut open_menu = use_signal(String::new);
    let mut legal_first_name = use_signal(String::new);
    let mut legal_last_name = use_signal(String::new);
    let mut guest_phone = use_signal(String::new);
    let mut guest_email = use_signal(String::new);
    let mut selected_room = use_signal(|| {
        crate::guest_pages::query_value("room_id")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0_u32)
    });
    let mut available = use_signal(Vec::<String>::new);
    let mut checking = use_signal(|| false);
    let mut submitting = use_signal(|| false);
    let mut message = use_signal(String::new);
    let mut confirmed = use_signal(|| None::<GuestBooking>);
    let mut copy_status = use_signal(String::new);

    use_effect(move || {
        spawn(async move {
            match tour_api::options().await {
                Ok(result) => {
                    if let Some(id) = locked_house_id {
                        if !result.houses.iter().any(|house| house.id == id) {
                            message.set("This house is not currently available for tours.".into());
                        }
                    }
                    options.set(Some(result));
                }
                Err(error) => message.set(error),
            }
        });
    });
    use_effect(move || {
        spawn(async move {
            if let Ok(response) = Request::get("/api/guest-listings").send().await {
                if let Ok(result) = response
                    .json::<Vec<crate::guest_model::GuestListing>>()
                    .await
                {
                    listings.set(result);
                }
            }
        });
    });
    let selected_house_name = options()
        .and_then(|data| {
            data.houses
                .into_iter()
                .find(|house| house.id == selected_house())
        })
        .map(|house| house.name)
        .unwrap_or_else(|| "Select a house".into());
    let house_rooms = options()
        .and_then(|data| data.houses.into_iter().find(|h| h.id == selected_house()))
        .map(|h| h.rooms)
        .unwrap_or_default();
    let occupied_rooms = listings()
        .into_iter()
        .find(|listing| listing.id == selected_house())
        .map(|listing| listing.rooms)
        .unwrap_or_default();
    let selected_room_open = occupied_rooms
        .iter()
        .find(|room| room.id == selected_room())
        .is_none_or(|room| room.is_available(rental_year, &rental_season, rental_months));
    let selected_time_name = if selected_slot().is_empty() {
        if checking() {
            "Checking times…"
        } else if selected_house() == 0 || tour_date().is_empty() {
            "Choose house and date first"
        } else if available().is_empty() {
            "No times available"
        } else {
            "Select a start time"
        }
    } else {
        slot_label(&selected_slot())
    };
    use_effect(move || {
        let house = selected_house();
        let date = tour_date();
        selected_slot.set(String::new());
        available.set(Vec::new());
        message.set(String::new());
        if house == 0 || date.is_empty() {
            checking.set(false);
            return;
        }
        checking.set(true);
        spawn(async move {
            let result = tour_api::availability(house, &date).await;
            if selected_house() == house && tour_date() == date {
                checking.set(false);
                match result {
                    Ok(result) => available.set(result.available_slots),
                    Err(error) => message.set(error),
                }
            }
        });
    });

    rsx! {
        document::Title { "Book a house tour | Buildry" }
        document::Stylesheet { href: crate::CSS }
        main { class: "booking-page",
            header { class: "booking-brand", a { class: "booking-brand-link", href: "/stays", img { class: "brand-symbol", src: "/icons/buildry-mark.svg?v=20261017-vibrant", alt: "" } span { "Buildry" } } }
            a { class: "booking-back-link back-icon-button", href: "{return_path}", aria_label: "Back to listing", title: "Back to listing", Icon { name: "arrow-left" } }
            section { class: "booking-card",
                if let Some(confirmation) = confirmed() {
                    div { class: "booking-confirmation",
                        div { class: "confirmation-icon", Icon { name: "check" } }
                        h1 { "Tour booked" }
                        p { "Thanks, {confirmation.booking.guest_name}. Your house tour is confirmed." }
                        div { class: "confirmation-details",
                            div { span { "House" } strong { "{confirmation.booking.property_name}" } }
                            div { span { "Email" } strong { "{confirmation.booking.guest_email}" } }
                            if let Some(room) = &confirmation.booking.room_name { div { span { "Room" } strong { "{room}" } } }
                            if let (Some(start), Some(months)) = (&confirmation.booking.rental_start, confirmation.booking.rental_months) {
                                div { span { "Rental period" } strong { "{display_rental_period(start, months, confirmation.booking.rental_year)}" } }
                            }
                            div { span { "Date" } strong { "{display_tour_date(&confirmation.booking.tour_date)}" } }
                            div { span { "Two-hour window" } strong { "{window_label(&confirmation.booking.slot_start)}" } }
                        }
                        div { class: "guest-manage-link",
                            strong { "Your booking link" }
                            p { "Save this private link to reschedule or cancel your tour later." }
                            a { href: "/booking/{confirmation.token}", "{manage_url(&confirmation.token)}" }
                            button { r#type: "button", class: "booking-submit", onclick: { let url = manage_url(&confirmation.token); move |_| {
                                let url = url.clone();
                                spawn(async move {
                                    match copy_link(url).await {
                                        Ok(()) => copy_status.set("Link copied!".into()),
                                        Err(error) => copy_status.set(error),
                                    }
                                });
                            } }, "Copy booking link" }
                            if !copy_status().is_empty() { span { class: "booking-copy-status", role: "status", "{copy_status}" } }
                        }
                        p { class: "booking-small", "Times are shown in Toronto time." }
                    }
                } else {
                    div { class: "booking-heading",
                        p { class: "booking-kicker", "HOUSE TOURS" }
                        h1 { "Book a house tour" }
                        p { "Confirm your room and selected rental period, then choose a time to visit. Your tour window will be two hours. These details can prepare a lease if you decide to rent." }
                    }
                    crate::app_form::AppForm { class: "booking-form", onsubmit: move |event: FormEvent| {
                        event.prevent_default();
                        if submitting() || selected_house() == 0 || selected_room() == 0 || !selected_room_open || tour_date().is_empty() || selected_slot().is_empty() { return; }
                        submitting.set(true);
                        message.set(String::new());
                        let request = NewTour {
                            property_id: selected_house(),
                            tour_date: tour_date(),
                            slot_start: selected_slot(),
                            legal_first_name: legal_first_name(),
                            legal_last_name: legal_last_name(),
                            guest_phone: guest_phone(),
                            guest_email:guest_email(),room_id:selected_room(),rental_start:rental_start.clone(),rental_months,
                        };
                        spawn(async move {
                            match tour_api::book(&request).await {
                                Ok(booking) => confirmed.set(Some(booking)),
                                Err(error) => {
                                    message.set(error);
                                    if let Ok(result) = tour_api::availability(request.property_id, &request.tour_date).await {
                                        available.set(result.available_slots);
                                    }
                                }
                            }
                            submitting.set(false);
                        });
                    },
                        div { class: "booking-choice",
                            span { class: "booking-choice-label", "House" }
                            button { r#type: "button", class: if locked_house_id.is_some() { "choice-trigger locked" } else if selected_house() == 0 { "choice-trigger placeholder" } else { "choice-trigger" }, aria_label: "House: {selected_house_name}", disabled: options().is_none() || locked_house_id.is_some(), onclick: move |_| open_menu.set("house".into()), "{selected_house_name}" }
                            if locked_house_id.is_none() && open_menu() == "house" {
                                button { r#type: "button", class: "choice-backdrop", aria_label: "Close house choices", onclick: move |_| open_menu.set(String::new()) }
                                div { class: "choice-popover",
                                    div { class: "choice-popover-head", strong { "Choose a house" } button { r#type: "button", aria_label: "Close house choices", onclick: move |_| open_menu.set(String::new()), Icon { name: "close" } } }
                                    div { class: "choice-popover-list",
                                        if let Some(data) = options() {
                                            for house in data.houses {
                                                button { key: "{house.id}", r#type: "button", class: if selected_house() == house.id { "choice-option selected" } else { "choice-option" }, onclick: move |_| { selected_house.set(house.id); selected_room.set(0); open_menu.set(String::new()); }, span { "{house.name}" } if selected_house() == house.id { span { class: "choice-check", Icon { name: "check" } } } }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        crate::form_select::FormSelect { label: "Room or suite", required: true, value: selected_room().to_string(), placeholder: "Choose a room", options: house_rooms.iter().map(|room|(room.id.to_string(),room.name.clone())).collect::<Vec<_>>(), disabled_values: house_rooms.iter().filter(|room|occupied_rooms.iter().find(|item|item.id==room.id).is_some_and(|item|!item.is_available(rental_year,&rental_season,rental_months))).map(|room|room.id.to_string()).collect::<Vec<_>>(), onchange: move |value:String| selected_room.set(value.parse().unwrap_or(0)) }
                        if !selected_room_open { p { class: "booking-error", "This room is occupied during the selected rental period. Change dates or choose another room." } }
                        div { class: "booking-rental-period", span { "Selected rental period" } strong { "{rental_label}" } a { href: "{return_path}", "Change dates" } }
                        div { class: "date-field", span { "Date" }
                            if let Some(data) = options() {
                                DatePicker { label: "Tour date", value: tour_date(), min: Some(data.today), max: Some(data.last_date), on_change: move |date| tour_date.set(date) }
                            } else {
                                DatePicker { label: "Tour date", value: String::new(), disabled: true, on_change: move |_| {} }
                            }
                        }
                        div { class: "booking-choice",
                            span { class: "booking-choice-label", "Start time" }
                            button { r#type: "button", class: if selected_slot().is_empty() { "choice-trigger placeholder" } else { "choice-trigger" }, aria_label: "Start time: {selected_time_name}", disabled: selected_house() == 0 || tour_date().is_empty() || checking() || available().is_empty(), onclick: move |_| open_menu.set("time".into()), "{selected_time_name}" }
                            if open_menu() == "time" {
                                button { r#type: "button", class: "choice-backdrop", aria_label: "Close time choices", onclick: move |_| open_menu.set(String::new()) }
                                div { class: "choice-popover",
                                    div { class: "choice-popover-head", strong { "Choose a start time" } button { r#type: "button", aria_label: "Close time choices", onclick: move |_| open_menu.set(String::new()), Icon { name: "close" } } }
                                    div { class: "choice-popover-list",
                                        for (key, label) in TOUR_SLOTS {
                                            if available().iter().any(|slot| slot == key) {
                                                button { key: "{key}", r#type: "button", class: if selected_slot() == key { "choice-option selected" } else { "choice-option" }, onclick: move |_| { selected_slot.set(key.into()); open_menu.set(String::new()); }, span { "{label}" } if selected_slot() == key { span { class: "choice-check", Icon { name: "check" } } } }
                                            }
                                        }
                                    }
                                }
                            }
                            if !selected_slot().is_empty() { p { class: "booking-window-note", "Your tour window: {window_label(&selected_slot())} (2 hours)" } }
                        }
                        div { class: "booking-fields",
                            label { "Legal first name *", crate::form_input::FormInput { required: true, autocomplete: "given-name", maxlength: "100", value: "{legal_first_name}", placeholder: "First name", oninput: move |event: FormEvent| legal_first_name.set(event.value()) } }
                            label { "Legal last name *", crate::form_input::FormInput { required: true, autocomplete: "family-name", maxlength: "100", value: "{legal_last_name}", placeholder: "Last name", oninput: move |event: FormEvent| legal_last_name.set(event.value()) } }
                            label { "Phone number *", crate::form_input::FormInput { r#type: "tel", required: true, autocomplete: "tel", maxlength: "30", value: "{guest_phone}", placeholder: "(519) 555-0123", oninput: move |event: FormEvent| guest_phone.set(event.value()) } }
                            label { class:"booking-email-field", "Email address *", crate::form_input::FormInput {r#type:"email",required:true,autocomplete:"email",maxlength:"254",value:"{guest_email}",placeholder:"you@example.com",oninput:move |event: FormEvent|guest_email.set(event.value())} small {"We’ll use this if you decide to rent."} }
                        }
                        if !message().is_empty() { p { class: "booking-error", role: "alert", "{message}" } }
                        button { class: "booking-submit", r#type: "submit", disabled: options().is_none() || submitting() || checking() || selected_house() == 0 || selected_room() == 0 || !selected_room_open || selected_slot().is_empty(), if submitting() { "Booking…" } else { "Book tour" } }
                        p { class: "booking-small", "Two-hour windows · Hourly starts 10 AM–6 PM · Toronto time" }
                    }
                }
            }
        }
    }
}

pub fn guest_manage_page() -> Element {
    let token = web_sys::window()
        .and_then(|window| window.location().pathname().ok())
        .and_then(|path| path.rsplit('/').next().map(str::to_string))
        .unwrap_or_default();
    let mut booking = use_signal(|| None::<GuestBookingStatus>);
    let mut options = use_signal(|| None::<TourOptions>);
    let mut loading = use_signal(|| true);
    let mut mode = use_signal(|| "view".to_string());
    let mut edit_date = use_signal(String::new);
    let mut edit_slot = use_signal(String::new);
    let mut available = use_signal(Vec::<String>::new);
    let mut checking = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut notice = use_signal(String::new);

    use_effect({
        let token = token.clone();
        move || {
            let token = token.clone();
            spawn(async move {
                match tour_api::guest_booking(&token).await {
                    Ok(result) => booking.set(Some(result)),
                    Err(problem) => error.set(problem),
                }
                if let Ok(result) = tour_api::options().await {
                    options.set(Some(result));
                }
                loading.set(false);
            });
        }
    });
    use_effect(move || {
        if mode() != "reschedule" {
            return;
        }
        let Some(current) = booking() else {
            return;
        };
        let date = edit_date();
        if date.is_empty() {
            return;
        }
        checking.set(true);
        spawn(async move {
            let result = tour_api::availability_for_reschedule(
                current.booking.property_id,
                &date,
                current.booking.id,
            )
            .await;
            if mode() == "reschedule"
                && booking()
                    .as_ref()
                    .is_some_and(|item| item.booking.id == current.booking.id)
                && edit_date() == date
            {
                checking.set(false);
                match result {
                    Ok(result) => {
                        if !edit_slot().is_empty() && !result.available_slots.contains(&edit_slot())
                        {
                            edit_slot.set(String::new());
                        }
                        available.set(result.available_slots);
                    }
                    Err(problem) => error.set(problem),
                }
            }
        });
    });

    rsx! {
        document::Title { "Manage your tour | Buildry" }
        document::Stylesheet { href: crate::CSS }
        main { class: "booking-page",
            header { class: "booking-brand", img { class: "brand-symbol", src: "/icons/buildry-mark.svg?v=20261017-vibrant", alt: "" } span { "Buildry" } }
            section { class: "booking-card guest-manage-card",
                if loading() {
                    p { class: "guest-manage-loading", "Loading your booking…" }
                } else if let Some(current) = booking() {
                    if current.cancelled {
                        div { class: "booking-confirmation",
                            div { class: "confirmation-icon", Icon { name: "check" } }
                            h1 { "Booking cancelled" }
                            p { "Your tour at {current.booking.property_name} has been cancelled." }
                            p { class: "booking-small", "The time is available for new bookings." }
                        }
                    } else if mode() == "reschedule" {
                        div { class: "guest-manage-heading",
                            p { class: "booking-kicker", "MANAGE BOOKING" }
                            h1 { "Reschedule your tour" }
                            p { "{current.booking.property_name}" }
                        }
                        crate::app_form::AppForm { class: "guest-manage-form", onsubmit: { let token = token.clone(); move |event: FormEvent| {
                            event.prevent_default();
                            if busy() || edit_slot().is_empty() { return; }
                            busy.set(true);
                            error.set(String::new());
                            let request = RescheduleTour { tour_date: edit_date(), slot_start: edit_slot() };
                            let token = token.clone();
                            spawn(async move {
                                match tour_api::guest_reschedule(&token, &request).await {
                                    Ok(updated) => {
                                        booking.set(Some(updated));
                                        mode.set("view".into());
                                        notice.set("Your tour has been rescheduled.".into());
                                    }
                                    Err(problem) => {
                                        error.set(problem);
                                        if let Ok(refreshed) = tour_api::guest_booking(&token).await {
                                            if let Ok(slots) = tour_api::availability_for_reschedule(
                                                refreshed.booking.property_id, &request.tour_date, refreshed.booking.id,
                                            ).await { available.set(slots.available_slots); }
                                        }
                                    }
                                }
                                busy.set(false);
                            });
                        } },
                            div { class: "tour-reschedule-date date-field", span { "New date" }
                                if let Some(data) = options() {
                                    DatePicker { label: "New tour date", value: edit_date(), min: Some(data.today), max: Some(data.last_date), on_change: move |date| { edit_date.set(date); edit_slot.set(String::new()); available.set(Vec::new()); error.set(String::new()); } }
                                }
                            }
                            span { class: "tour-detail-label", "START TIME · TWO-HOUR WINDOW" }
                            if checking() { p { class: "tour-reschedule-hint", "Checking available times…" } }
                            else if available().is_empty() { p { class: "tour-reschedule-hint", "No times available for this date." } }
                            else {
                                div { class: "tour-reschedule-times",
                                    for (key, label) in TOUR_SLOTS {
                                        if available().iter().any(|slot| slot == key) {
                                            button { key: "{key}", r#type: "button", class: if edit_slot() == key { "tour-time-choice selected" } else { "tour-time-choice" }, onclick: move |_| edit_slot.set(key.into()), "{label}" }
                                        }
                                    }
                                }
                            }
                            if !edit_slot().is_empty() { p { class: "tour-reschedule-hint", "New window: {window_label(&edit_slot())}" } }
                            if !error().is_empty() { p { class: "booking-error", role: "alert", "{error}" } }
                            div { class: "guest-manage-actions",
                                button { r#type: "button", class: "back-icon-button", aria_label: "Back to booking", title: "Back to booking", disabled: busy(), onclick: move |_| { mode.set("view".into()); error.set(String::new()); }, Icon { name: "arrow-left" } }
                                button { r#type: "submit", class: "booking-submit", disabled: busy() || checking() || edit_slot().is_empty(), if busy() { "Saving…" } else { "Save new time" } }
                            }
                        }
                    } else if mode() == "cancel" {
                        div { class: "guest-manage-heading",
                            p { class: "booking-kicker", "MANAGE BOOKING" }
                            h1 { "Cancel your tour?" }
                            p { "{current.booking.property_name}" }
                        }
                        div { class: "guest-manage-summary",
                            strong { "{display_tour_date(&current.booking.tour_date)}" }
                            span { "{window_label(&current.booking.slot_start)}" }
                        }
                        p { class: "guest-cancel-copy", "This will release your booking. You can use the booking page to choose another time later." }
                        if !error().is_empty() { p { class: "booking-error", role: "alert", "{error}" } }
                        div { class: "guest-manage-actions",
                            button { r#type: "button", class: "outline-button", disabled: busy(), onclick: move |_| { mode.set("view".into()); error.set(String::new()); }, "Keep booking" }
                            button { r#type: "button", class: "tour-danger-button", disabled: busy(), onclick: { let token = token.clone(); move |_| {
                                busy.set(true);
                                error.set(String::new());
                                let token = token.clone();
                                spawn(async move {
                                    match tour_api::guest_cancel(&token).await {
                                        Ok(()) => {
                                            if let Some(mut current) = booking() {
                                                current.cancelled = true;
                                                booking.set(Some(current));
                                            }
                                        }
                                        Err(problem) => error.set(problem),
                                    }
                                    busy.set(false);
                                });
                            } }, if busy() { "Cancelling…" } else { "Cancel tour" } }
                        }
                    } else {
                        div { class: "guest-manage-heading",
                            p { class: "booking-kicker", "YOUR BOOKING" }
                            h1 { "Your house tour" }
                            p { "Booked for {current.booking.guest_name}" }
                        }
                        if !notice().is_empty() { p { class: "guest-manage-notice", role: "status", "{notice}" } }
                        div { class: "guest-manage-summary",
                            span { "HOUSE" }
                            strong { "{current.booking.property_name}" }
                        }
                        if let Some(room) = &current.booking.room_name {
                            div { class: "guest-manage-summary", span { "ROOM" } strong { "{room}" } }
                        }
                        if let (Some(start), Some(months)) = (&current.booking.rental_start, current.booking.rental_months) {
                            div { class: "guest-manage-summary",
                                span { "RENTAL PERIOD" }
                                strong { "{display_rental_period(start, months, current.booking.rental_year)}" }
                            }
                        }
                        div { class: "guest-manage-summary",
                            span { "WHEN · TORONTO TIME" }
                            strong { "{display_tour_date(&current.booking.tour_date)}" }
                            strong { class: "guest-manage-time", "{window_label(&current.booking.slot_start)}" }
                        }
                        div { class: "guest-manage-actions",
                            button { r#type: "button", class: "booking-submit", onclick: { let date = current.booking.tour_date.clone(); let slot = current.booking.slot_start.clone(); move |_| {
                                let today = options().map(|item| item.today).unwrap_or_default();
                                let initial = if date < today { today } else { date.clone() };
                                edit_date.set(initial.clone());
                                edit_slot.set(if initial == date { slot.clone() } else { String::new() });
                                available.set(Vec::new());
                                error.set(String::new());
                                mode.set("reschedule".into());
                            } }, "Reschedule" }
                            button { r#type: "button", class: "guest-cancel-button", onclick: move |_| { error.set(String::new()); mode.set("cancel".into()); }, "Cancel booking" }
                        }
                    }
                } else {
                    div { class: "guest-manage-heading",
                        h1 { "Booking link not found" }
                        p { "Check the link you received after booking." }
                    }
                    if !error().is_empty() { p { class: "booking-error", role: "alert", "{error}" } }
                }
            }
        }
    }
}

pub fn dashboard_page() -> Element {
    let CalendarUiState {
        mut today,
        mut week_offset,
        selected_day: mut selected_agenda_day,
        start_offset: mut agenda_start_offset,
        end_offset: mut agenda_end_offset,
        compact: mut agenda_compact,
        search_open: mut calendar_search_open,
        search: mut calendar_search,
    } = use_context::<CalendarUiState>();
    let mut bookings = use_signal(Vec::<TourBooking>::new);
    let mut housework = use_signal(Vec::<HouseworkTicket>::new);
    let mut houses = use_signal(Vec::<TourHouse>::new);
    let mut houses_loading = use_signal(|| true);
    let mut house_busy = use_signal(|| None::<u32>);
    let mut house_error = use_signal(String::new);
    let mut last_date = use_signal(String::new);
    let mut now_time = use_signal(String::new);
    let mut message = use_signal(String::new);
    let mut agenda_initialized = use_signal(|| false);
    let mut agenda_prepending = use_signal(|| false);
    let mut agenda_scroll_origin = use_signal(|| None::<i32>);
    let mut selected_booking = use_signal(|| None::<TourBooking>);
    let mut detail_mode = use_signal(|| "view".to_string());
    let mut edit_date = use_signal(String::new);
    let mut edit_slot = use_signal(String::new);
    let mut edit_available = use_signal(Vec::<String>::new);
    let mut edit_loading = use_signal(|| false);
    let mut detail_busy = use_signal(|| false);
    let mut detail_error = use_signal(String::new);
    let mut action_status = use_signal(String::new);
    use_effect(move || {
        spawn(async move {
            match tour_api::list().await {
                Ok(result) => bookings.set(result),
                Err(error) => message.set(error),
            }
        });
    });
    use_effect(move || {
        spawn(async move {
            if let Ok(response) = Request::get("/api/tasks").send().await {
                if let Ok(items) = response.json::<Vec<HouseworkTicket>>().await {
                    housework.set(items);
                }
            }
        });
    });
    use_effect(move || {
        spawn(async move {
            match tour_api::houses().await {
                Ok(result) => houses.set(result),
                Err(error) => house_error.set(error),
            }
            houses_loading.set(false);
        });
    });
    use_effect(move || {
        spawn(async move {
            loop {
                if let Ok(result) = tour_api::options().await {
                    today.set(result.today);
                    last_date.set(result.last_date);
                    now_time.set(result.now_time);
                }
                TimeoutFuture::new(60_000).await;
            }
        });
    });
    use_effect(move || {
        let date = today();
        if !date.is_empty() && !agenda_initialized() {
            agenda_initialized.set(true);
            let selected = selected_agenda_day();
            if selected.is_empty() {
                selected_agenda_day.set(date.clone());
                scroll_to_agenda_date(date);
            } else {
                scroll_to_agenda_date(selected);
            }
        }
    });
    use_effect(move || {
        let date = selected_agenda_day();
        if !date.is_empty() {
            scroll_to_agenda_week_date(date);
        }
    });
    use_effect(move || {
        if detail_mode() != "reschedule" {
            return;
        }
        let Some(booking) = selected_booking() else {
            return;
        };
        let date = edit_date();
        if date.is_empty() {
            return;
        }
        edit_loading.set(true);
        spawn(async move {
            let result =
                tour_api::availability_for_reschedule(booking.property_id, &date, booking.id).await;
            if detail_mode() == "reschedule"
                && selected_booking()
                    .as_ref()
                    .is_some_and(|current| current.id == booking.id)
                && edit_date() == date
            {
                edit_loading.set(false);
                match result {
                    Ok(available) => {
                        if !edit_slot().is_empty()
                            && !available.available_slots.contains(&edit_slot())
                        {
                            edit_slot.set(String::new());
                        }
                        edit_available.set(available.available_slots);
                    }
                    Err(error) => detail_error.set(error),
                }
            }
        });
    });
    let all_bookings = bookings();
    let scheduled_work: Vec<_> = housework()
        .into_iter()
        .filter(|item| {
            matches!(item.status.as_str(), "planned" | "in_progress") && item.start_date.is_some()
        })
        .collect();
    let now_line_top = now_time()
        .split_once(':')
        .and_then(|(hour, minute)| {
            Some(hour.parse::<i32>().ok()? * 60 + minute.parse::<i32>().ok()?)
        })
        .filter(|minutes| (600..1200).contains(minutes))
        .map(|minutes| (minutes - 600) * 70 / 60);
    let mut calendar_days = Vec::new();
    let mut week_label = "Loading week…".to_string();
    if let Ok(current) = NaiveDate::parse_from_str(&today(), "%Y-%m-%d") {
        let monday = current - Duration::days(current.weekday().num_days_from_monday() as i64)
            + Duration::weeks(week_offset());
        let sunday = monday + Duration::days(6);
        week_label = format!(
            "{} – {}",
            monday.format("%b %-d"),
            sunday.format("%b %-d, %Y")
        );
        for offset in 0..7 {
            let date = monday + Duration::days(offset);
            let date_text = date.format("%Y-%m-%d").to_string();
            let mut day_bookings: Vec<_> = all_bookings
                .iter()
                .filter(|booking| booking.tour_date == date_text)
                .cloned()
                .collect();
            day_bookings.sort_by(|left, right| left.slot_start.cmp(&right.slot_start));
            calendar_days.push(CalendarDay {
                date: date_text,
                weekday: date.format("%a").to_string(),
                day_number: date.format("%-d").to_string(),
                is_today: date == current,
                events: layout_events(day_bookings),
            });
        }
    }
    let mut agenda_days = Vec::new();
    if let Ok(current) = NaiveDate::parse_from_str(&today(), "%Y-%m-%d") {
        for offset in agenda_start_offset()..=agenda_end_offset() {
            let date = current + Duration::days(offset);
            let date_text = date.format("%Y-%m-%d").to_string();
            let mut day_bookings: Vec<_> = all_bookings
                .iter()
                .filter(|booking| booking.tour_date == date_text)
                .cloned()
                .collect();
            day_bookings.sort_by(|left, right| left.slot_start.cmp(&right.slot_start));
            agenda_days.push(CalendarDay {
                date: date_text,
                weekday: date.format("%A").to_string(),
                day_number: date.format("%-d").to_string(),
                is_today: date == current,
                events: layout_events(day_bookings),
            });
        }
    }
    let show_now = calendar_days.iter().any(|day| day.is_today);
    let week_dates: Vec<_> = calendar_days.iter().map(|day| day.date.clone()).collect();
    let mut week_work: Vec<_> = scheduled_work
        .iter()
        .filter(|item| {
            item.start_date
                .as_ref()
                .is_some_and(|date| week_dates.contains(date))
        })
        .cloned()
        .collect();
    week_work.sort_by(|a, b| {
        a.start_date
            .cmp(&b.start_date)
            .then(a.start_time.cmp(&b.start_time))
    });
    let open_house_count = houses().iter().filter(|house| house.available).count();
    let house_count = houses().len();
    let tour_houses_initially_open = use_memo(|| {
        web_sys::window()
            .and_then(|window| window.inner_width().ok())
            .and_then(|width| width.as_f64())
            .map_or(true, |width| width > 680.0)
    });
    let search_term = calendar_search().to_lowercase();
    let mut search_results: Vec<_> = all_bookings
        .iter()
        .filter(|booking| {
            search_term.is_empty()
                || format!(
                    "{} {} {}",
                    booking.guest_name, booking.property_name, booking.tour_date
                )
                .to_lowercase()
                .contains(&search_term)
        })
        .cloned()
        .collect();
    search_results.sort_by(|left, right| {
        left.tour_date
            .cmp(&right.tour_date)
            .then(left.slot_start.cmp(&right.slot_start))
    });
    rsx! {
        document::Title { "Calendar | Buildry" }
        document::Stylesheet { href: crate::CSS }
        document::Script { src: "/assets/tour-detail-swipe.mjs?v=2", r#type: "module" }
        crate::admin_shell::AdminShell { active: "/calendar", property_count: None, main_class: "main-content tours-main",
                section { class: "page-head",
                    h1 { "Calendar" }
                }
                if !action_status().is_empty() { p { class: "tour-copy-status", role: "status", "{action_status}" } }
                if !message().is_empty() { p { class: "booking-error", "{message}" } }
                CollapsiblePanel {
                    title: "Tour availability".to_string(),
                    subtitle: None,
                    badge: Some(format!("{open_house_count} / {house_count} open")),
                    class: "tour-houses-panel".to_string(),
                    initially_open: tour_houses_initially_open(),
                    if houses_loading() { p { class: "tour-houses-empty", "Loading houses…" } }
                    else if houses().is_empty() && house_error().is_empty() { p { class: "tour-houses-empty", "No houses found." } }
                    else {
                        div { class: "tour-houses-list",
                            for house in houses() {
                                    button { key: "{house.id}",
                                        r#type: "button",
                                        class: if house.available { "tour-house-choice available" } else { "tour-house-choice" },
                                        aria_label: "House {house.id}, {house.name}, {house.address} tours",
                                        aria_pressed: house.available,
                                        disabled: house_busy().is_some(),
                                        onclick: { let house_id = house.id; let next = !house.available; move |_| {
                                            house_busy.set(Some(house_id));
                                            house_error.set(String::new());
                                            spawn(async move {
                                                match tour_api::set_house_availability(house_id, next).await {
                                                    Ok(updated) => {
                                                        let mut current = houses();
                                                        if let Some(item) = current.iter_mut().find(|item| item.id == house_id) { *item = updated; }
                                                        houses.set(current);
                                                    }
                                                    Err(error) => house_error.set(error),
                                                }
                                                house_busy.set(None);
                                            });
                                        } },
                                        span { class: "tour-house-toggle-track", span { class: "tour-house-toggle-thumb" } }
                                        span { class: "tour-house-name", "House {house.id} · {house.name} · {house.address}" }
                                        if house_busy() == Some(house.id) { span { class: "tour-house-saving", "…" } }
                                    }
                            }
                        }
                    }
                    if !house_error().is_empty() { p { class: "booking-error", role: "alert", "{house_error}" } }
                }
                section { class:"tour-agenda-mobile",aria_label:"Tour agenda",
                    if calendar_search_open() {
                        div {class:"tour-agenda-search-results",
                            if search_results.is_empty(){p {class:"tour-agenda-empty","No tours match that search."}}
                            for booking in search_results {button {key:"{booking.id}",r#type:"button",class:"tour-agenda-search-result",onclick:{let date=booking.tour_date.clone();move |_|{calendar_search_open.set(false);calendar_search.set(String::new());jump_to_agenda_date(date.clone(),&today(),selected_agenda_day,agenda_start_offset,agenda_end_offset,week_offset)}},
                                strong {"{booking.property_name}"} small {"{booking.guest_name} · {agenda_date(&booking.tour_date)} · {window_label(&booking.slot_start)}"}
                            }}
                        }
                    } else {
                        div {id:"tour-agenda-list",class:"tour-agenda-list",onscroll:move |_|{
                            let Some(document)=web_sys::window().and_then(|window|window.document())else{return};
                            let Some(list)=document.get_element_by_id("tour-agenda-list")else{return};
                            let top=list.scroll_top();let height=list.client_height();let total=list.scroll_height();
                            if agenda_scroll_origin().is_none() && top>0 {agenda_scroll_origin.set(Some(top))}
                            let should_compact=agenda_scroll_origin().is_some_and(|origin|top>origin+30);
                            if agenda_compact()!=should_compact{agenda_compact.set(should_compact)}
                            if total-top-height<600{agenda_end_offset.set(agenda_end_offset()+30)}
                            if top<300&&!agenda_prepending(){
                                agenda_prepending.set(true);let old_total=total;let old_top=top;
                                agenda_start_offset.set(agenda_start_offset()-30);
                                spawn(async move {TimeoutFuture::new(35).await;if let Some(list)=web_sys::window().and_then(|window|window.document()).and_then(|document|document.get_element_by_id("tour-agenda-list")){list.set_scroll_top(old_top+list.scroll_height()-old_total)}agenda_prepending.set(false)});
                            }
                            let rect=list.get_bounding_client_rect();
                            let visible_top=document.query_selector(".calendar-topbar").ok().flatten().map(|header|header.get_bounding_client_rect().bottom()).unwrap_or(rect.top()).max(rect.top());
                            let Some(element)=document.element_from_point((rect.left()+24.0) as f32,(visible_top+24.0) as f32)else{return};
                            let Some(section)=element.closest(".tour-agenda-day").ok().flatten()else{return};
                            let Some(date)=section.get_attribute("data-agenda-date")else{return};
                            if selected_agenda_day()!=date{
                                selected_agenda_day.set(date.clone());
                                ripple_auto_selected_agenda_date(date.clone());
                                if let (Ok(current),Ok(day))=(NaiveDate::parse_from_str(&today(),"%Y-%m-%d"),NaiveDate::parse_from_str(&date,"%Y-%m-%d")){
                                    let first=current-Duration::days(current.weekday().num_days_from_sunday() as i64);
                                    let second=day-Duration::days(day.weekday().num_days_from_sunday() as i64);
                                    week_offset.set((second-first).num_days()/7);
                                }
                            }
                        },for day in &agenda_days {section {key:"{day.date}",id:"agenda-{day.date}","data-agenda-date":"{day.date}",class:"tour-agenda-day",
                            h3 {"{day.weekday} · {agenda_date(&day.date)}" if day.is_today{small {"Today"}}}
                            if day.events.is_empty() && !scheduled_work.iter().any(|item| item.start_date.as_deref() == Some(day.date.as_str())) {p {class:"tour-agenda-empty","Nothing scheduled"}}
                            for item in scheduled_work.iter().filter(|item| item.start_date.as_deref() == Some(day.date.as_str())) { a { key: "work-{item.id}", class: "tour-agenda-event housework-agenda-event", href: format!("/tasks/{}", item.id), span { class: "tour-agenda-time", strong { "{item.start_time.as_deref().unwrap_or_default()}" } small { "{item.estimate_minutes.unwrap_or(0)} min" } } span { class: "tour-agenda-marker" } span { class: "tour-agenda-copy", strong { "{item.title}" } small { "Tasks · {item.property_name}" } } } }
                            for event in &day.events {button {key:"{event.booking.id}",r#type:"button",class:"tour-agenda-event",onclick:{let booking=event.booking.clone();move |_|{detail_mode.set("view".into());detail_error.set(String::new());selected_booking.set(Some(booking.clone()));}},
                                span {class:"tour-agenda-time",strong {"{slot_label(&event.booking.slot_start)}"} small {"2 hr"}}
                                span {class:"tour-agenda-marker"}
                                span {class:"tour-agenda-copy",strong {"{event.booking.property_name}"} small {"{event.booking.guest_name}"}}}
                            }
                        }}}
                    }
                }
                section { class: "tour-calendar-panel",
                    div { class: "tour-calendar-head",
                        h2 { "{week_label}" }
                        div { class: "tour-calendar-controls",
                            button { r#type: "button", aria_label: "Previous week", onclick: move |_| week_offset.set(week_offset() - 1), Icon { name: "chevron-left" } }
                            button { r#type: "button", class: "tour-today-button", onclick: move |_| week_offset.set(0), "Today" }
                            button { r#type: "button", aria_label: "Next week", onclick: move |_| week_offset.set(week_offset() + 1), Icon { name: "chevron-right" } }
                        }
                    }
                    if !week_work.is_empty() { div { class: "housework-calendar-strip", h3 { "Tasks this week" } div { for item in week_work { a { key: "work-{item.id}", href: format!("/tasks/{}", item.id), span { class: "housework-calendar-time", "{item.start_date.as_deref().unwrap_or_default()} · {item.start_time.as_deref().unwrap_or_default()}" } strong { "{item.title}" } small { "{item.property_name} · {item.estimate_minutes.unwrap_or(0)} min" } } } } } }
                    div { class: "tour-calendar-scroll",
                        div { class: "tour-calendar-grid",
                            div { class: "tour-time-column",
                                div { class: "tour-time-spacer" }
                                div { class: "tour-time-rail",
                                    for (hour, label) in [(0, "10 AM"), (1, "11 AM"), (2, "12 PM"), (3, "1 PM"), (4, "2 PM"), (5, "3 PM"), (6, "4 PM"), (7, "5 PM"), (8, "6 PM"), (9, "7 PM"), (10, "8 PM")] {
                                        span { key: "{hour}", style: "top:{hour * 70 - 7}px", "{label}" }
                                    }
                                    if show_now {
                                        if let Some(top) = now_line_top { span { class: "tour-now-label", style: "top:{top - 10}px", "NOW" } }
                                    }
                                }
                            }
                            for day in calendar_days {
                                div { key: "{day.date}", class: if day.is_today { "tour-calendar-day today" } else { "tour-calendar-day" },
                                    div { class: "tour-calendar-date", span { "{day.weekday}" } strong { "{day.day_number}" } }
                                    div { class: "tour-calendar-track",
                                        if day.is_today {
                                            if let Some(top) = now_line_top { div { class: "tour-now-line", style: "top:{top}px" } }
                                        }
                                        for event in day.events {
                                            button { key: "{event.booking.id}", class: "tour-calendar-booking", style: "{event.style}", r#type: "button", onclick: { let booking = event.booking.clone(); move |_| { detail_mode.set("view".into()); detail_error.set(String::new()); action_status.set(String::new()); selected_booking.set(Some(booking.clone())); } },
                                                strong { "{window_label(&event.booking.slot_start)}" }
                                                span { "{admin_house_label(&houses(), event.booking.property_id, &event.booking.property_name)}" }
                                                small { "{event.booking.guest_name}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

        }
        if let Some(booking) = selected_booking() {
            div { class: "tour-detail-backdrop glass-dialog-backdrop", role: "dialog", aria_modal: "true", aria_label: "Tour details",
                div { class: "tour-detail-card glass-dialog-card",
                    div { class: "tour-detail-top",
                        span { class: "tour-detail-drag-handle", aria_hidden: "true" }
                        div { class: "tour-detail-topline",
                            span { class: "tour-detail-eyebrow", "HOUSE TOUR" }
                            button { class:"glass-icon-button",r#type: "button", aria_label: "Close tour details", onclick: move |_| selected_booking.set(None), Icon { name: "close" } }
                        }
                        h2 { "Tour details" }
                        crate::form_action_bar::TourPartySummary {
                            image: crate::guest_pages::house_photo(booking.property_id),
                            property_name: booking.property_name.clone(),
                            property_subtitle: booking.room_name.clone().unwrap_or_else(|| houses().iter().find(|house| house.id == booking.property_id).map(|house| house.address.clone()).unwrap_or_default()),
                            guest_name: booking.guest_name.clone(),
                            guest_email: booking.guest_email.clone(),
                            guest_phone: booking.guest_phone.clone(),
                        }
                        if let (Some(start), Some(months)) = (&booking.rental_start, booking.rental_months) {
                            div {class:"tour-rental-summary",crate::form_action_bar::StaySummary {start:start.clone(),months:months as u32}}
                        }
                    }
                    div { class: "tour-detail-content",
                        if detail_mode() == "reschedule" {
                            crate::app_form::AppForm { class: "tour-reschedule-form", onsubmit: { let booking_id = booking.id; let property_id = booking.property_id; move |event: FormEvent| {
                                event.prevent_default();
                                if detail_busy() || edit_slot().is_empty() { return; }
                                detail_busy.set(true);
                                detail_error.set(String::new());
                                let request = RescheduleTour { tour_date: edit_date(), slot_start: edit_slot() };
                                spawn(async move {
                                    match tour_api::reschedule(booking_id, &request).await {
                                        Ok(updated) => {
                                            let mut current = bookings();
                                            if let Some(item) = current.iter_mut().find(|item| item.id == booking_id) { *item = updated.clone(); }
                                            bookings.set(current);
                                            if let (Ok(target), Ok(today_date)) = (
                                                NaiveDate::parse_from_str(&updated.tour_date, "%Y-%m-%d"),
                                                NaiveDate::parse_from_str(&today(), "%Y-%m-%d"),
                                            ) {
                                                let target_monday = target - Duration::days(target.weekday().num_days_from_monday() as i64);
                                                let current_monday = today_date - Duration::days(today_date.weekday().num_days_from_monday() as i64);
                                                week_offset.set((target_monday - current_monday).num_days() / 7);
                                            }
                                            selected_booking.set(None);
                                            detail_mode.set("view".into());
                                            action_status.set("Tour rescheduled.".into());
                                        }
                                        Err(error) => {
                                            detail_error.set(error);
                                            if let Ok(available) = tour_api::availability_for_reschedule(
                                                property_id,
                                                &request.tour_date,
                                                booking_id,
                                            ).await { edit_available.set(available.available_slots); }
                                        }
                                    }
                                    detail_busy.set(false);
                                });
                            } },
                                h3 { "Reschedule tour" }
                                div { class: "tour-reschedule-date date-field", span { "New date" }
                                    DatePicker { label: "New tour date", value: edit_date(), min: Some(today()), max: Some(last_date()), on_change: move |date| { edit_date.set(date); edit_slot.set(String::new()); edit_available.set(Vec::new()); detail_error.set(String::new()); } }
                                }
                                span { class: "tour-detail-label", "START TIME · TWO-HOUR WINDOW" }
                                if edit_loading() { p { class: "tour-reschedule-hint", "Checking available times…" } }
                                else if edit_available().is_empty() { p { class: "tour-reschedule-hint", "No times available for this date." } }
                                else {
                                    div { class: "tour-reschedule-times",
                                        for (key, label) in TOUR_SLOTS {
                                            if edit_available().iter().any(|slot| slot == key) {
                                                button { key: "{key}", r#type: "button", class: if edit_slot() == key { "tour-time-choice selected" } else { "tour-time-choice" }, onclick: move |_| edit_slot.set(key.into()), "{label}" }
                                            }
                                        }
                                    }
                                }
                                if !edit_slot().is_empty() { p { class: "tour-reschedule-hint", "New window: {window_label(&edit_slot())}" } }
                                if !detail_error().is_empty() { p { class: "booking-error", role: "alert", "{detail_error}" } }
                                div { class: "tour-detail-actions",
                                    button { r#type: "button", class: "back-icon-button", aria_label: "Back to tour details", title: "Back to tour details", disabled: detail_busy(), onclick: move |_| { detail_mode.set("view".into()); detail_error.set(String::new()); }, Icon { name: "arrow-left" } }
                                    button { r#type: "submit", class: "glass-icon-button confirm", aria_label:"Save new time",title:"Save new time",disabled: detail_busy() || edit_loading() || edit_slot().is_empty(), Icon {name:"check"} }
                                }
                            }
                        } else if detail_mode() == "cancel" {
                            div { class: "tour-cancel-confirm",
                                h3 { "Cancel this tour?" }
                                strong { class: "tour-cancel-summary", "{display_tour_date(&booking.tour_date)} · {window_label(&booking.slot_start)}" }
                                p { "This booking will leave your calendar and the time will become available again." }
                                if !detail_error().is_empty() { p { class: "booking-error", role: "alert", "{detail_error}" } }
                                div { class: "tour-detail-actions",
                                    button { r#type: "button", class: "glass-icon-button",aria_label:"Keep tour",title:"Keep tour", disabled: detail_busy(), onclick: move |_| { detail_mode.set("view".into()); detail_error.set(String::new()); },Icon {name:"arrow-left"} }
                                    button { r#type: "button", class: "glass-icon-button danger",aria_label:"Cancel tour",title:"Cancel tour", disabled: detail_busy(), onclick: { let booking_id = booking.id; move |_| {
                                        detail_busy.set(true);
                                        detail_error.set(String::new());
                                        spawn(async move {
                                            match tour_api::cancel(booking_id).await {
                                                Ok(()) => {
                                                    bookings.set(bookings().into_iter().filter(|item| item.id != booking_id).collect());
                                                    selected_booking.set(None);
                                                    detail_mode.set("view".into());
                                                    action_status.set("Tour cancelled.".into());
                                                }
                                                Err(error) => detail_error.set(error),
                                            }
                                            detail_busy.set(false);
                                        });
                                    } },Icon {name:"close"} }
                                }
                            }
                        } else {
                            div { class: "tour-detail-schedule",
                                span { class: "tour-detail-label", "WHEN" }
                                strong { class: "tour-detail-date", "{display_tour_date(&booking.tour_date)}" }
                                strong { class: "tour-detail-time", "{window_label(&booking.slot_start)}" }
                            }
                            if let Some(manager) = &booking.manager_name {
                                div { class: "tour-detail-host",
                                    span { class: "tour-detail-label", "TOUR MANAGER" }
                                    strong { "{manager}" }
                                    if booking.manager_notice_error.as_deref() == Some("legacy") { small { class: "tour-detail-notice", "Booked before email notices were enabled" } }
                                    else if booking.manager_notice_sent { small { class: "tour-detail-notice sent", "Booking email sent" } }
                                    else if let Some(error) = &booking.manager_notice_error { small { class: "tour-detail-notice pending", "Email pending · {error}" } }
                                    else { small { class: "tour-detail-notice pending", "Booking email pending" } }
                                }
                            }
                            div { class: "tour-detail-actions tour-detail-view-actions",
                                button { r#type: "button", class: "glass-icon-button",aria_label:"Reschedule",title:"Reschedule", onclick: { let booking_id = booking.id; let booking_date = booking.tour_date.clone(); let booking_slot = booking.slot_start.clone(); move |_| {
                                    let initial_date = if booking_date < today() { today() } else { booking_date.clone() };
                                    edit_date.set(initial_date.clone());
                                    edit_slot.set(if initial_date == booking_date { booking_slot.clone() } else { String::new() });
                                    edit_available.set(Vec::new());
                                    detail_error.set(String::new());
                                    if selected_booking().as_ref().is_some_and(|item| item.id == booking_id) { detail_mode.set("reschedule".into()); }
                                } },Icon {name:"calendar"} }
                                a {class:"primary-button tour-create-lease",href:"/leases/new?tour={booking.id}","Create lease",Icon { name: "arrow-right" }}
                            }
                            button { r#type: "button", class: "tour-cancel-link tour-detail-cancel", onclick: move |_| { detail_error.set(String::new()); detail_mode.set("cancel".into()); }, "Cancel tour" }
                        }
                    }
                }
            }
        }
    }
}
