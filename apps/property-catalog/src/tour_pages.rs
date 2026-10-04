use crate::{
    guest_model::{rental_period_label, term_label},
    tour_api,
    tour_model::{
        slot_label, window_label, GuestBooking, GuestBookingStatus, NewTour, RescheduleTour,
        TourBooking, TourHouse, TourOptions, TOUR_SLOTS,
    },
};
use chrono::{Datelike, Duration, NaiveDate};
use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
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

pub fn guest_page() -> Element {
    let return_path = booking_return_path();
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
    let mut selected_house = use_signal(|| locked_house_id.unwrap_or(0));
    let mut tour_date = use_signal(String::new);
    let mut selected_slot = use_signal(String::new);
    let mut open_menu = use_signal(String::new);
    let mut guest_name = use_signal(String::new);
    let mut guest_phone = use_signal(String::new);
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
    let selected_house_name = options()
        .and_then(|data| {
            data.houses
                .into_iter()
                .find(|house| house.id == selected_house())
        })
        .map(|house| house.name)
        .unwrap_or_else(|| "Select a house".into());
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
            header { class: "booking-brand", a { class: "booking-brand-link", href: "/stays", span { class: "brand-mark", "B" } span { "buildry" } } }
            a { class: "booking-back-link", href: "{return_path}", "← Back to listing" }
            section { class: "booking-card",
                if let Some(confirmation) = confirmed() {
                    div { class: "booking-confirmation",
                        div { class: "confirmation-icon", "✓" }
                        h1 { "Tour booked" }
                        p { "Thanks, {confirmation.booking.guest_name}. Your house tour is confirmed." }
                        div { class: "confirmation-details",
                            div { span { "House" } strong { "{confirmation.booking.property_name}" } }
                            if let Some(room) = &confirmation.booking.room_name { div { span { "Room" } strong { "{room}" } } }
                            if let (Some(start), Some(months)) = (&confirmation.booking.rental_start, confirmation.booking.rental_months) {
                                div { span { "Rental period" } strong { "{confirmation.booking.rental_year.map(|year| rental_period_label(year, start, months)).unwrap_or_else(|| term_label(start, months).unwrap_or(\"Selected term\").into())}" } }
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
                        p { "Choose a house and a time to visit. Your tour window will be two hours. We'll use your phone number to coordinate the visit." }
                    }
                    form { class: "booking-form", onsubmit: move |event| {
                        event.prevent_default();
                        if submitting() || selected_house() == 0 || tour_date().is_empty() || selected_slot().is_empty() { return; }
                        submitting.set(true);
                        message.set(String::new());
                        let request = NewTour {
                            property_id: selected_house(),
                            tour_date: tour_date(),
                            slot_start: selected_slot(),
                            guest_name: guest_name(),
                            guest_phone: guest_phone(),
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
                                    div { class: "choice-popover-head", strong { "Choose a house" } button { r#type: "button", aria_label: "Close house choices", onclick: move |_| open_menu.set(String::new()), "×" } }
                                    div { class: "choice-popover-list",
                                        if let Some(data) = options() {
                                            for house in data.houses {
                                                button { key: "{house.id}", r#type: "button", class: if selected_house() == house.id { "choice-option selected" } else { "choice-option" }, onclick: move |_| { selected_house.set(house.id); open_menu.set(String::new()); }, span { "{house.name}" } if selected_house() == house.id { span { class: "choice-check", "✓" } } }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        label { "Date",
                            if let Some(data) = options() {
                                input { r#type: "date", required: true, min: "{data.today}", max: "{data.last_date}", value: "{tour_date}", oninput: move |event| tour_date.set(event.value()) }
                            } else {
                                input { r#type: "date", disabled: true }
                            }
                        }
                        div { class: "booking-choice",
                            span { class: "booking-choice-label", "Start time" }
                            button { r#type: "button", class: if selected_slot().is_empty() { "choice-trigger placeholder" } else { "choice-trigger" }, aria_label: "Start time: {selected_time_name}", disabled: selected_house() == 0 || tour_date().is_empty() || checking() || available().is_empty(), onclick: move |_| open_menu.set("time".into()), "{selected_time_name}" }
                            if open_menu() == "time" {
                                button { r#type: "button", class: "choice-backdrop", aria_label: "Close time choices", onclick: move |_| open_menu.set(String::new()) }
                                div { class: "choice-popover",
                                    div { class: "choice-popover-head", strong { "Choose a start time" } button { r#type: "button", aria_label: "Close time choices", onclick: move |_| open_menu.set(String::new()), "×" } }
                                    div { class: "choice-popover-list",
                                        for (key, label) in TOUR_SLOTS {
                                            if available().iter().any(|slot| slot == key) {
                                                button { key: "{key}", r#type: "button", class: if selected_slot() == key { "choice-option selected" } else { "choice-option" }, onclick: move |_| { selected_slot.set(key.into()); open_menu.set(String::new()); }, span { "{label}" } if selected_slot() == key { span { class: "choice-check", "✓" } } }
                                            }
                                        }
                                    }
                                }
                            }
                            if !selected_slot().is_empty() { p { class: "booking-window-note", "Your tour window: {window_label(&selected_slot())} (2 hours)" } }
                        }
                        div { class: "booking-fields",
                            label { "Your name", input { required: true, autocomplete: "name", maxlength: "100", value: "{guest_name}", placeholder: "Full name", oninput: move |event| guest_name.set(event.value()) } }
                            label { "Phone number", input { r#type: "tel", required: true, autocomplete: "tel", maxlength: "30", value: "{guest_phone}", placeholder: "(519) 555-0123", oninput: move |event| guest_phone.set(event.value()) } }
                        }
                        if !message().is_empty() { p { class: "booking-error", role: "alert", "{message}" } }
                        button { class: "booking-submit", r#type: "submit", disabled: options().is_none() || submitting() || checking() || selected_house() == 0 || selected_slot().is_empty(), if submitting() { "Booking…" } else { "Book tour" } }
                        p { class: "booking-small", "Two-hour windows · Hourly starts 10 AM–4 PM · Toronto time" }
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
            header { class: "booking-brand", span { class: "brand-mark", "B" } span { "buildry" } }
            section { class: "booking-card guest-manage-card",
                if loading() {
                    p { class: "guest-manage-loading", "Loading your booking…" }
                } else if let Some(current) = booking() {
                    if current.cancelled {
                        div { class: "booking-confirmation",
                            div { class: "confirmation-icon", "✓" }
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
                        form { class: "guest-manage-form", onsubmit: { let token = token.clone(); move |event| {
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
                            label { class: "tour-reschedule-date", "New date",
                                if let Some(data) = options() {
                                    input { r#type: "date", required: true, min: "{data.today}", max: "{data.last_date}", value: "{edit_date}", oninput: move |event| { edit_date.set(event.value()); edit_slot.set(String::new()); available.set(Vec::new()); error.set(String::new()); } }
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
                                button { r#type: "button", class: "outline-button", disabled: busy(), onclick: move |_| { mode.set("view".into()); error.set(String::new()); }, "Back" }
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
                                strong { "{current.booking.rental_year.map(|year| rental_period_label(year, start, months)).unwrap_or_else(|| term_label(start, months).unwrap_or(\"Selected term\").into())}" }
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
    let mut bookings = use_signal(Vec::<TourBooking>::new);
    let mut houses = use_signal(Vec::<TourHouse>::new);
    let mut houses_loading = use_signal(|| true);
    let mut house_busy = use_signal(|| None::<u32>);
    let mut house_error = use_signal(String::new);
    let mut today = use_signal(String::new);
    let mut last_date = use_signal(String::new);
    let mut now_time = use_signal(String::new);
    let mut message = use_signal(String::new);
    let mut week_offset = use_signal(|| 0_i64);
    let mut sidebar_collapsed =
        use_signal(|| LocalStorage::get::<bool>("buildry.sidebar.collapsed").unwrap_or(false));
    let mut selected_booking = use_signal(|| None::<TourBooking>);
    let mut detail_mode = use_signal(|| "view".to_string());
    let mut edit_date = use_signal(String::new);
    let mut edit_slot = use_signal(String::new);
    let mut edit_available = use_signal(Vec::<String>::new);
    let mut edit_loading = use_signal(|| false);
    let mut detail_busy = use_signal(|| false);
    let mut detail_error = use_signal(String::new);
    let mut action_status = use_signal(String::new);
    let mut copy_status = use_signal(String::new);
    let guest_url = web_sys::window()
        .and_then(|window| window.location().origin().ok())
        .map(|origin| format!("{origin}/book"))
        .unwrap_or_else(|| "/book".into());
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
    let now_line_top = now_time()
        .split_once(':')
        .and_then(|(hour, minute)| {
            Some(hour.parse::<i32>().ok()? * 60 + minute.parse::<i32>().ok()?)
        })
        .filter(|minutes| (600..1080).contains(minutes))
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
    let show_now = calendar_days.iter().any(|day| day.is_today);
    let open_house_count = houses().iter().filter(|house| house.available).count();
    let house_count = houses().len();
    let copied_url = guest_url.clone();
    rsx! {
        document::Title { "House tours | Buildry" }
        document::Stylesheet { href: crate::CSS }
        div { class: "app-shell",
            aside { class: if sidebar_collapsed() { "sidebar collapsed" } else { "sidebar" },
                div { class: "sidebar-header",
                    div { class: "brand", span { class: "brand-mark", "B" } span { class: "brand-name", "buildry" } }
                    button { class: "sidebar-toggle", r#type: "button", aria_label: if sidebar_collapsed() { "Expand sidebar" } else { "Collapse sidebar" }, onclick: move |_| { let next = !sidebar_collapsed(); sidebar_collapsed.set(next); let _ = LocalStorage::set("buildry.sidebar.collapsed", next); }, if sidebar_collapsed() { "›" } else { "‹" } }
                }
                a { class: "nav-item tour-nav-link", href: "/", title: "Properties", span { class: "nav-icon icon-properties" } span { class: "nav-text", "Properties" } }
                a { class: "nav-item tour-nav-link", href: "/pricing", title: "Pricing", span { class: "nav-icon icon-pricing" } span { class: "nav-text", "Pricing" } }
                a { class: "nav-item tour-nav-link active", href: "/tours", title: "House tours", span { class: "nav-icon icon-tours" } span { class: "nav-text", "House tours" } }
                a { class: "nav-item tour-nav-link", href: "/availability", title: "Availability", span { class: "nav-icon icon-availability" } span { class: "nav-text", "Availability" } }
                a { class: "nav-item tour-nav-link", href: "/stays", title: "Guest stays", span { class: "nav-icon icon-stays" } span { class: "nav-text", "Guest stays" } }
                form { class: "nav-logout", method: "post", action: "/logout", button { class: "nav-item tour-nav-link", r#type: "submit", title: "Sign out", span { class: "nav-icon icon-logout" } span { class: "nav-text", "Sign out" } } }
            }
            main { class: "main-content tours-main",
                section { class: "page-head",
                    h1 { "House tours" }
                    div { class: "tour-page-actions",
                        a { class: "outline-button tour-open-button", href: "/book", target: "_blank", "Open guest page ↗" }
                        button { class: "primary-button", r#type: "button", onclick: move |_| {
                            let url = copied_url.clone();
                            spawn(async move {
                                match copy_link(url).await {
                                    Ok(()) => copy_status.set("Copied!".into()),
                                    Err(error) => copy_status.set(error),
                                }
                            });
                        }, "Copy booking link" }
                    }
                }
                if !copy_status().is_empty() { p { class: "tour-copy-status", role: "status", "{copy_status}" } }
                if !action_status().is_empty() { p { class: "tour-copy-status", role: "status", "{action_status}" } }
                if !message().is_empty() { p { class: "booking-error", "{message}" } }
                section { class: "tour-houses-panel", aria_label: "Tour availability",
                    div { class: "tour-houses-heading",
                        h2 { "Tour availability" }
                        span { class: "tour-houses-count", "{open_house_count} / {house_count} open" }
                    }
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
                section { class: "tour-calendar-panel",
                    div { class: "tour-calendar-head",
                        h2 { "{week_label}" }
                        div { class: "tour-calendar-controls",
                            button { r#type: "button", aria_label: "Previous week", onclick: move |_| week_offset.set(week_offset() - 1), "‹" }
                            button { r#type: "button", class: "tour-today-button", onclick: move |_| week_offset.set(0), "Today" }
                            button { r#type: "button", aria_label: "Next week", onclick: move |_| week_offset.set(week_offset() + 1), "›" }
                        }
                    }
                    div { class: "tour-calendar-scroll",
                        div { class: "tour-calendar-grid",
                            div { class: "tour-time-column",
                                div { class: "tour-time-spacer" }
                                div { class: "tour-time-rail",
                                    for (hour, label) in [(0, "10 AM"), (1, "11 AM"), (2, "12 PM"), (3, "1 PM"), (4, "2 PM"), (5, "3 PM"), (6, "4 PM"), (7, "5 PM"), (8, "6 PM")] {
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
                                            button { key: "{event.booking.id}", class: if event.booking.is_demo { "tour-calendar-booking demo" } else { "tour-calendar-booking" }, style: "{event.style}", r#type: "button", onclick: { let booking = event.booking.clone(); move |_| { detail_mode.set("view".into()); detail_error.set(String::new()); action_status.set(String::new()); selected_booking.set(Some(booking.clone())); } },
                                                strong { "{window_label(&event.booking.slot_start)}" }
                                                span { "{admin_house_label(&houses(), event.booking.property_id, &event.booking.property_name)}" }
                                                small { "{event.booking.guest_name}" }
                                                if event.booking.is_demo { em { "Sample" } }
                                            }
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
            div { class: "tour-detail-backdrop", role: "dialog", aria_modal: "true", aria_label: "Tour details",
                div { class: "tour-detail-card",
                    div { class: "tour-detail-top",
                        div { class: "tour-detail-topline",
                            span { class: "tour-detail-eyebrow", "HOUSE TOUR" }
                            button { r#type: "button", aria_label: "Close tour details", onclick: move |_| selected_booking.set(None), "×" }
                        }
                        h2 { "{admin_house_label(&houses(), booking.property_id, &booking.property_name)}" }
                        if let Some(room) = &booking.room_name { p { "Room: {room}" } }
                        if let (Some(start), Some(months), Some(rent)) = (&booking.rental_start, booking.rental_months, booking.quoted_monthly_rent) {
                            p { "{booking.rental_year.map(|year| rental_period_label(year, start, months)).unwrap_or_else(|| term_label(start, months).unwrap_or(\"Selected term\").into())} · C${rent}/month" if booking.quote_is_estimate { " · example rate" } }
                        }
                        if booking.is_demo { span { class: "tour-sample-badge", "Sample booking" } }
                    }
                    div { class: "tour-detail-content",
                        if detail_mode() == "reschedule" {
                            form { class: "tour-reschedule-form", onsubmit: { let booking_id = booking.id; let property_id = booking.property_id; move |event| {
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
                                label { class: "tour-reschedule-date", "New date",
                                    input { r#type: "date", required: true, min: "{today}", max: "{last_date}", value: "{edit_date}", oninput: move |event| { edit_date.set(event.value()); edit_slot.set(String::new()); edit_available.set(Vec::new()); detail_error.set(String::new()); } }
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
                                    button { r#type: "button", class: "outline-button", disabled: detail_busy(), onclick: move |_| { detail_mode.set("view".into()); detail_error.set(String::new()); }, "Back" }
                                    button { r#type: "submit", class: "primary-button", disabled: detail_busy() || edit_loading() || edit_slot().is_empty(), if detail_busy() { "Saving…" } else { "Save new time" } }
                                }
                            }
                        } else if detail_mode() == "cancel" {
                            div { class: "tour-cancel-confirm",
                                h3 { "Cancel this tour?" }
                                strong { class: "tour-cancel-summary", "{display_tour_date(&booking.tour_date)} · {window_label(&booking.slot_start)}" }
                                p { "This booking will leave your calendar and the time will become available again." }
                                if !detail_error().is_empty() { p { class: "booking-error", role: "alert", "{detail_error}" } }
                                div { class: "tour-detail-actions",
                                    button { r#type: "button", class: "outline-button", disabled: detail_busy(), onclick: move |_| { detail_mode.set("view".into()); detail_error.set(String::new()); }, "Keep tour" }
                                    button { r#type: "button", class: "tour-danger-button", disabled: detail_busy(), onclick: { let booking_id = booking.id; move |_| {
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
                                    } }, if detail_busy() { "Cancelling…" } else { "Cancel tour" } }
                                }
                            }
                        } else {
                            div { class: "tour-detail-schedule",
                                span { class: "tour-detail-label", "WHEN" }
                                strong { class: "tour-detail-date", "{display_tour_date(&booking.tour_date)}" }
                                strong { class: "tour-detail-time", "{window_label(&booking.slot_start)}" }
                            }
                            div { class: "tour-detail-guest",
                                span { class: "tour-detail-label", "GUEST" }
                                strong { "{booking.guest_name}" }
                                a { href: "tel:{booking.guest_phone}", class: "tour-detail-phone", "Call {booking.guest_phone} ↗" }
                            }
                            div { class: "tour-detail-actions tour-detail-view-actions",
                                button { r#type: "button", class: "outline-button", onclick: { let booking_id = booking.id; let booking_date = booking.tour_date.clone(); let booking_slot = booking.slot_start.clone(); move |_| {
                                    let initial_date = if booking_date < today() { today() } else { booking_date.clone() };
                                    edit_date.set(initial_date.clone());
                                    edit_slot.set(if initial_date == booking_date { booking_slot.clone() } else { String::new() });
                                    edit_available.set(Vec::new());
                                    detail_error.set(String::new());
                                    if selected_booking().as_ref().is_some_and(|item| item.id == booking_id) { detail_mode.set("reschedule".into()); }
                                } }, "Reschedule" }
                                button { r#type: "button", class: "tour-cancel-link", onclick: move |_| { detail_error.set(String::new()); detail_mode.set("cancel".into()); }, "Cancel booking" }
                            }
                        }
                    }
                }
            }
        }
    }
}
