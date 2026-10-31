use crate::guest_model::{period_keys, rental_period_label, GuestListing};

fn breadcrumb_label(name: &str) -> String {
    const LIMIT: usize = 32;
    if name.chars().count() <= LIMIT {
        name.to_string()
    } else {
        format!(
            "{}…",
            name.chars().take(LIMIT).collect::<String>().trim_end()
        )
    }
}

#[cfg(test)]
mod breadcrumb_tests {
    use super::breadcrumb_label;

    #[test]
    fn long_names_shorten_without_changing_detail_titles() {
        let title = "A welcoming private room with ensuite in Waterloo";
        assert_eq!(breadcrumb_label(title), "A welcoming private room with en…");
        assert_eq!(breadcrumb_label("Cozy room"), "Cozy room");
    }
}
use crate::icon::Icon;
use dioxus::prelude::*;
use gloo_net::http::Request;
use gloo_timers::future::TimeoutFuture;
use serde::Deserialize;

static HOUSE_PHOTO: Asset = asset!("/assets/stay-house.png");
static HOUSE_BRICK_PHOTO: Asset = asset!("/assets/stay-house-brick.jpg");
static HOUSE_MODERN_PHOTO: Asset = asset!("/assets/stay-house-modern.jpg");
static HOUSE_TOWNHOME_PHOTO: Asset = asset!("/assets/stay-house-townhome.jpg");
static HOUSE_RED_BRICK_PHOTO: Asset = asset!("/assets/stay-house-red-brick.jpg");
static HOUSE_CREAM_PHOTO: Asset = asset!("/assets/stay-house-cream.jpg");
static BASEMENT_ENSUITE_1_PHOTO: Asset = asset!("/assets/stay-basement-ensuite-1.jpg");
static BASEMENT_ENSUITE_2_PHOTO: Asset = asset!("/assets/stay-basement-ensuite-2.jpg");
static BASEMENT_SUITE_PHOTO: Asset = asset!("/assets/stay-basement-suite.jpg");
static MASTER_ENSUITE_PHOTO: Asset = asset!("/assets/stay-master-ensuite.jpg");
static REGULAR_BEDROOM_1_PHOTO: Asset = asset!("/assets/stay-regular-bedroom-1.jpg");
static REGULAR_BEDROOM_2_PHOTO: Asset = asset!("/assets/stay-regular-bedroom-2.jpg");
static STANDARD_ENSUITE_1_PHOTO: Asset = asset!("/assets/stay-standard-ensuite-1.jpg");
static STANDARD_ENSUITE_2_PHOTO: Asset = asset!("/assets/stay-standard-ensuite-2.jpg");

#[derive(Clone, PartialEq)]
struct StayCard {
    key: String,
    property_id: u32,
    room_id: Option<u32>,
    nearby_intersection: Option<String>,
    title: String,
    area: String,
    category: &'static str,
    price: u32,
    image: String,
    photos: Vec<String>,
    href: String,
    tours_available: bool,
    available: bool,
    available_rooms: usize,
}

#[derive(Deserialize)]
struct GuestPlaceResult {
    matching_property_ids: Vec<u32>,
    label: String,
}

pub(crate) fn query_value(name: &str) -> Option<String> {
    let query = web_sys::window()?.location().search().ok()?;
    query.trim_start_matches('?').split('&').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        if key == name {
            urlencoding::decode(value)
                .ok()
                .map(|value| value.into_owned())
        } else {
            None
        }
    })
}

pub(crate) fn default_period() -> (i32, String, u8) {
    let date = js_sys::Date::new_0();
    let year = date.get_full_year() as i32;
    let month = date.get_month();
    let (year, start) = if month < 4 {
        (year, "may")
    } else if month < 8 {
        (year, "sep")
    } else {
        (year + 1, "jan")
    };
    let chosen_year = query_value("year")
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|value| (*value - year).abs() <= 8)
        .unwrap_or(year);
    let chosen_start = query_value("start")
        .filter(|value| ["sep", "jan", "may"].contains(&value.as_str()))
        .unwrap_or_else(|| start.into());
    let months = query_value("months")
        .and_then(|value| value.parse::<u8>().ok())
        .filter(|value| [4, 8, 12, 24, 36].contains(value))
        .unwrap_or(4);
    (chosen_year, chosen_start, months)
}

fn period_query(year: i32, start: &str, months: u8) -> String {
    format!("year={year}&start={start}&months={months}")
}

fn open_period(
    rooms: &[crate::guest_model::GuestRoom],
    year: i32,
    start: &str,
    months: u8,
) -> Option<(i32, String, u8)> {
    if rooms
        .iter()
        .any(|room| room.is_available(year, start, months))
    {
        return Some((year, start.into(), months));
    }
    let today = js_sys::Date::new_0();
    let first = (today.get_full_year() as i32 * 12 + today.get_month() as i32).max(
        year * 12
            + match start {
                "jan" => 0,
                "may" => 4,
                _ => 8,
            },
    );
    for next_year in first.div_euclid(12)..=first.div_euclid(12) + 9 {
        for (season, month) in [("jan", 0), ("may", 4), ("sep", 8)] {
            if next_year * 12 + month < first {
                continue;
            }
            if rooms
                .iter()
                .any(|room| room.is_available(next_year, season, 4))
            {
                return Some((next_year, season.into(), 4));
            }
        }
    }
    None
}

fn fallback_price(property_id: u32, room_index: usize) -> u32 {
    900 + ((property_id as usize + room_index * 2) % 6) as u32 * 75
}

fn price_label(price: u32) -> String {
    if price >= 1_000 {
        format!("C${},{:03}", price / 1_000, price % 1_000)
    } else {
        format!("C${price}")
    }
}

#[component]
fn RentalCalendar(
    rental_year: Signal<i32>,
    rental_start: Signal<String>,
    rental_months: Signal<u8>,
    rooms: Vec<crate::guest_model::GuestRoom>,
    compact: bool,
) -> Element {
    let mut rental_year = rental_year;
    let mut rental_start = rental_start;
    let mut rental_months = rental_months;
    let mut view_year = use_signal(|| rental_year());
    let mut choosing_end = use_signal(|| false);
    let first_month = match rental_start().as_str() {
        "jan" => 0,
        "may" => 4,
        _ => 8,
    };
    let first = rental_year() * 12 + first_month;
    let last = first + rental_months() as i32 - 1;
    let earliest_year = js_sys::Date::new_0().get_full_year() as i32;
    let latest_year = earliest_year + 9;
    let month_names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let calendar = rsx! {
        div { class: "rental-calendar",
            div { class: "rental-calendar-head",
                button { r#type: "button", aria_label: "Previous year", disabled: view_year() <= earliest_year, onclick: move |_| view_year.set(view_year() - 1), Icon { name: "chevron-left" } }
                strong { "{view_year()}" }
                button { r#type: "button", aria_label: "Next year", disabled: view_year() >= latest_year, onclick: move |_| view_year.set(view_year() + 1), Icon { name: "chevron-right" } }
            }
            div { class: "rental-calendar-months", role: "group", aria_label: "Rental calendar",
                for index in 0..12_i32 {
                    {
                        let absolute = view_year() * 12 + index;
                        let is_start_month = index == 0 || index == 4 || index == 8;
                        let is_end_month = index == 3 || index == 7 || index == 11;
                        let month_open = rooms.iter().any(|room| room.is_month_range_available(view_year(), index as u32 + 1, 1));
                        let start_open = is_start_month && rooms.iter().any(|room| room.is_month_range_available(view_year(), index as u32 + 1, 4));
                        let duration = absolute - first + 1;
                        let end_open = choosing_end() && is_end_month && [4, 8, 12, 24, 36].contains(&duration)
                            && rooms.iter().any(|room| room.is_available(rental_year(), &rental_start(), duration as u8));
                        let enabled = (start_open || end_open) && absolute >= earliest_year * 12;
                        let in_range = absolute >= first && absolute <= last;
                        let blocked = !month_open || (is_start_month && !start_open) || (choosing_end() && is_end_month && absolute >= first && !end_open);
                        let class = if blocked { "occupied" } else if absolute == first { "range-start" } else if !choosing_end() && absolute == last { "range-end" } else if !choosing_end() && in_range { "in-range" } else { "" };
                        let month = month_names[index as usize];
                        rsx! {
                            button { key: "{view_year()}-{index}", r#type: "button", class: "{class}", disabled: !enabled,
                                aria_label: if blocked { format!("{month} {} unavailable", view_year()) } else { format!("{month} {}", view_year()) },
                                onclick: move |_| {
                                    if end_open {
                                        rental_months.set(duration as u8);
                                        choosing_end.set(false);
                                    } else if start_open {
                                        rental_year.set(view_year());
                                        rental_start.set(match index { 0 => "jan", 4 => "may", _ => "sep" }.into());
                                        rental_months.set(4);
                                        choosing_end.set(true);
                                    }
                                },
                                "{month}"
                            }
                        }
                    }
                }
            }
            p { class: "rental-calendar-hint", if choosing_end() { "Now select an end month, or choose a stay length below." } else { "Select a start month, then an end month. Shaded dates are unavailable." } }
            div { class: "rental-calendar-length", role: "group", aria_label: "Stay length",
                for choice in [4_u8, 8, 12, 24, 36] {
                    {
                    let open = rooms.iter().any(|room| room.is_available(rental_year(), &rental_start(), choice));
                    let label = match choice { 12 => "1 yr".to_string(), 24 => "2 yrs".to_string(), 36 => "3 yrs".to_string(), _ => format!("{choice} mo") };
                    rsx! { button { key: "{choice}", r#type: "button", class: if rental_months() == choice { "active" } else { "" },
                        disabled: !open, aria_label: "{choice} months", onclick: move |_| { rental_months.set(choice); choosing_end.set(false); },
                        "{label}"
                    } }
                    }
                }
            }
            p { class: "rental-calendar-selected", "{rental_period_label(rental_year(), &rental_start(), rental_months())}" }
        }
    };
    if compact {
        let period_label = rental_period_label(rental_year(), &rental_start(), rental_months());
        let short_period_label = period_label.split(" · ").next().unwrap_or(&period_label);
        rsx! { crate::collapsible_panel::CollapsibleCard { class: "rental-calendar-shell compact", summary: rsx! { span { "Rental dates" } strong { class: "rental-period-full", "{period_label}" } strong { class: "rental-period-short", "{short_period_label}" } span { class: "rental-calendar-caret", aria_hidden: "true" } },
            {calendar}
        } }
    } else {
        rsx! { div { class: "rental-calendar-shell", {calendar} } }
    }
}

pub(crate) fn house_photo(property_id: u32) -> String {
    match property_id {
        1 => HOUSE_PHOTO,
        2 => HOUSE_BRICK_PHOTO,
        3 => HOUSE_TOWNHOME_PHOTO,
        4 => HOUSE_MODERN_PHOTO,
        5 => HOUSE_RED_BRICK_PHOTO,
        6 => HOUSE_CREAM_PHOTO,
        _ => HOUSE_PHOTO,
    }
    .to_string()
}

fn is_ai_illustration(url: &str) -> bool {
    url.contains("?illustrative=ai")
}

fn room_photo(room: &crate::guest_model::GuestRoom, index: usize) -> String {
    if let Some(photo) = room.photos.first() {
        return photo.clone();
    }
    let name = room.name.to_lowercase();
    let alternate = index % 2 == 1;
    let photo = if room.kind == "Suite" || name.contains("basement suite") {
        BASEMENT_SUITE_PHOTO
    } else if name.contains("basement") && room.bathroom_access == "Ensuite" {
        if alternate {
            BASEMENT_ENSUITE_2_PHOTO
        } else {
            BASEMENT_ENSUITE_1_PHOTO
        }
    } else if (name.contains("master") || name.contains("primary"))
        && room.bathroom_access == "Ensuite"
    {
        MASTER_ENSUITE_PHOTO
    } else if room.bathroom_access == "Ensuite" || name.contains("ensuite") {
        if alternate {
            STANDARD_ENSUITE_2_PHOTO
        } else {
            STANDARD_ENSUITE_1_PHOTO
        }
    } else if alternate {
        REGULAR_BEDROOM_2_PHOTO
    } else {
        REGULAR_BEDROOM_1_PHOTO
    };
    photo.to_string()
}

fn cards_for(listings: Vec<GuestListing>, year: i32, start: &str, months: u8) -> Vec<StayCard> {
    let mut houses = Vec::new();
    let mut rooms_cards = Vec::new();
    for listing in listings {
        let rooms: Vec<_> = if listing.rooms.is_empty() {
            vec![crate::guest_model::GuestRoom {
                id: 0,
                name: "Private room".into(),
                kind: "Bedroom".into(),
                bathroom_access: String::new(),
                photos: Vec::new(),
                monthly_price: None,
                price_is_estimate: true,
                rent_sep_dec: None,
                rent_jan_apr: None,
                rent_may_aug: None,
                price_curve: Vec::new(),
                monthly_price_overrides: Vec::new(),
                amenities: Vec::new(),
                occupied_periods: Vec::new(),
            }]
        } else {
            listing.rooms.clone()
        };
        let available_rooms = rooms
            .iter()
            .filter(|room| room.is_available(year, start, months))
            .count();
        let lowest = rooms
            .iter()
            .filter(|room| available_rooms == 0 || room.is_available(year, start, months))
            .enumerate()
            .map(|(index, room)| {
                room.curve_monthly_rent(year, start, months)
                    .or_else(|| room.lowest_monthly_rent())
                    .unwrap_or_else(|| fallback_price(listing.id, index))
            })
            .min()
            .unwrap_or(950);
        houses.push(StayCard {
            key: format!("house-{}", listing.id),
            property_id: listing.id,
            room_id: None,
            nearby_intersection: listing.nearby_intersection.clone(),
            title: listing.name.clone(),
            area: listing.area.clone(),
            category: "House",
            price: lowest,
            image: listing
                .photos
                .first()
                .cloned()
                .unwrap_or_else(|| house_photo(listing.id)),
            photos: listing.photos.clone(),
            href: format!(
                "/stays/house/{}?{}",
                listing.id,
                period_query(year, start, months)
            ),
            tours_available: listing.tours_available,
            available: available_rooms > 0,
            available_rooms,
        });
        for (index, room) in rooms.into_iter().enumerate() {
            let available = room.is_available(year, start, months);
            let price = room
                .curve_monthly_rent(year, start, months)
                .or_else(|| room.lowest_monthly_rent())
                .unwrap_or_else(|| fallback_price(listing.id, index));
            rooms_cards.push(StayCard {
                key: format!("room-{}-{}", listing.id, room.id),
                property_id: listing.id,
                room_id: if room.id == 0 { None } else { Some(room.id) },
                nearby_intersection: listing.nearby_intersection.clone(),
                title: room.name.clone(),
                area: listing.area.clone(),
                category: if room.kind == "Suite" {
                    "Suite"
                } else {
                    "Private room"
                },
                price,
                image: room_photo(&room, index),
                photos: room.photos.clone(),
                href: format!(
                    "/stays/room/{}/{}?{}",
                    listing.id,
                    room.id,
                    period_query(year, start, months)
                ),
                tours_available: listing.tours_available,
                available,
                available_rooms: usize::from(available),
            });
        }
    }
    houses.extend(rooms_cards);
    houses
}

fn tour_url(card: &StayCard) -> String {
    tour_url_from(card, &card.href)
}

fn tour_url_from(card: &StayCard, return_to: &str) -> String {
    let room = card
        .room_id
        .map(|id| format!("&room_id={id}"))
        .unwrap_or_default();
    let period = card.href.split('?').nth(1).unwrap_or_default();
    format!(
        "/book?property_id={}{}&{}&return_to={}",
        card.property_id,
        room,
        period,
        urlencoding::encode(return_to)
    )
}

pub fn guest_stays_page() -> Element {
    let (initial_year, initial_start, initial_months) = default_period();
    let mut listings = use_signal(Vec::<GuestListing>::new);
    let mut rental_year = use_signal(|| initial_year);
    let mut rental_start = use_signal(|| initial_start);
    let mut rental_months = use_signal(|| initial_months);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut search = use_signal(String::new);
    let mut filter = use_signal(|| "Full houses".to_string());
    let mut place_results = use_signal(|| None::<Vec<u32>>);
    let mut place_label = use_signal(String::new);
    let mut search_sequence = use_signal(|| 0_u32);
    let mut contact_open = use_signal(|| false);
    let mut mobile_search_open = use_signal(|| false);

    use_effect(move || {
        spawn(async move {
            match Request::get("/api/guest-listings").send().await {
                Ok(response) if response.ok() => match response.json::<Vec<GuestListing>>().await {
                    Ok(result) => {
                        let rooms: Vec<_> = result
                            .iter()
                            .flat_map(|listing| listing.rooms.iter().cloned())
                            .collect();
                        if let Some((year, start, months)) =
                            open_period(&rooms, rental_year(), &rental_start(), rental_months())
                        {
                            rental_year.set(year);
                            rental_start.set(start);
                            rental_months.set(months);
                        }
                        listings.set(result);
                    }
                    Err(_) => {
                        error.set("We couldn't load the stays. Please refresh the page.".into())
                    }
                },
                _ => error.set("We couldn't load the stays. Please refresh the page.".into()),
            }
            loading.set(false);
        });
    });

    let query = search().trim().to_lowercase();
    let active_filter = filter();
    let mut cards: Vec<StayCard> =
        cards_for(listings(), rental_year(), &rental_start(), rental_months())
            .into_iter()
            .filter(|card| {
                ((active_filter == "Full houses" && card.category == "House")
                    || (active_filter == "Rooms" && card.category != "House"))
                    && (query.is_empty()
                        || match place_results() {
                            Some(ids) if !ids.is_empty() => ids.contains(&card.property_id),
                            _ => {
                                card.title.to_lowercase().contains(&query)
                                    || card.area.to_lowercase().contains(&query)
                                    || card.nearby_intersection.as_ref().is_some_and(
                                        |intersection| intersection.to_lowercase().contains(&query),
                                    )
                            }
                        })
            })
            .collect();
    cards.sort_by_key(|card| !card.available);
    let current_period = period_query(rental_year(), &rental_start(), rental_months());
    let stays_return = format!("/stays?{current_period}");

    rsx! {
        document::Title { "Waterloo homes and rooms | Buildry" }
        document::Meta { name: "description", content: "Explore homes and private rooms in Waterloo, compare monthly prices, and book a house tour." }
        document::Stylesheet { href: crate::CSS }
        main { class: "stays-page",
            header { class: "stays-header",
                div { class: "stays-header-inner",
                    a { class: "stays-brand", href: "/stays", img { class: "brand-symbol", src: "/icons/buildry-mark.svg?v=20261017-vibrant", alt: "" } span { "Buildry" } }
                    nav { aria_label: "Guest navigation",
                        a { href: "/login", "Sign in" }
                    }
                }
            }
            section { class: "stays-content", id: "stays",
                div { class: "stays-toolbar",
                    h2 { "Explore stays" }
                    button { class: "stays-mobile-search-trigger", r#type: "button", onclick: move |_| mobile_search_open.set(true),
                        Icon { name: "search" }
                        span { strong { if search().trim().is_empty() { "Where to?" } else { "{search}" } } small { "{rental_period_label(rental_year(), &rental_start(), rental_months())} · {filter()}" } }
                    }
                    if mobile_search_open() { button { class: "stays-search-backdrop", r#type: "button", aria_label: "Close search", onclick: move |_| mobile_search_open.set(false) } }
                    div { class: if mobile_search_open() { "stays-search-panel open" } else { "stays-search-panel" },
                        div { class: "stays-search-panel-head", strong { "Search stays" } button { r#type: "button", aria_label: "Close search", onclick: move |_| mobile_search_open.set(false), Icon { name: "close" } } }
                        label { class: "stays-search",
                            span { "Where" }
                            crate::form_input::FormInput { r#type: "search", placeholder: "City, university, or landmark", value: "{search}", oninput: move |event: FormEvent| {
                            let value = event.value();
                            search.set(value.clone());
                            place_results.set(None);
                            place_label.set(String::new());
                            search_sequence += 1;
                            let sequence = search_sequence();
                            if value.trim().len() < 3 { return; }
                            spawn(async move {
                                TimeoutFuture::new(350).await;
                                if search_sequence() != sequence { return; }
                                let url = format!("/api/guest-place-search?q={}", urlencoding::encode(&value));
                                if let Ok(response) = Request::get(&url).send().await {
                                    if response.ok() {
                                        if let Ok(result) = response.json::<GuestPlaceResult>().await {
                                            if search_sequence() == sequence {
                                                place_results.set(Some(result.matching_property_ids));
                                                place_label.set(result.label);
                                            }
                                        }
                                    }
                                }
                            });
                            } }
                        }
                        div { class: "stays-search-dates",
                            RentalCalendar { rental_year, rental_start, rental_months, rooms: listings().into_iter().flat_map(|listing| listing.rooms).collect(), compact: true }
                        }
                        label { class: "stays-filters",
                            span { "Stay type" }
                            crate::form_input::FormNativeSelect { class: "stays-filter-select", value: "{filter()}", onchange: move |event: FormEvent| filter.set(event.value()),
                                option { value: "Full houses", "Full houses" }
                                option { value: "Rooms", "Rooms" }
                            }
                        }
                        button { class: "stays-mobile-search-done", r#type: "button", onclick: move |_| mobile_search_open.set(false), "Show stays" }
                    }
                }
                if !place_label().is_empty() { p { class: "stays-search-location", "Showing stays near {place_label()}" } }
                if loading() { p { class: "stays-state", role: "status", "Loading stays…" } }
                if !error().is_empty() { p { class: "stays-state", role: "alert", "{error}" } }
                if !loading() && error().is_empty() && cards.is_empty() { p { class: "stays-state", "No stays match your search." } }
                div { class: "stays-grid",
                    for card in cards {
                        article { key: "{card.key}", class: if card.available { "stay-card" } else { "stay-card stay-card-unavailable" },
                            a { class: "stay-card-image", href: "{card.href}", aria_label: "View {card.title} in {card.area}",
                                img { src: "{card.image}", alt: if card.category == "House" { "House exterior" } else { "Furnished room interior" } }
                                span { class: "stay-card-type", "{card.category}" }
                                if is_ai_illustration(&card.image) { span { class: "stay-card-illustrative", "AI illustration" } }
                                span { class: if card.available { "stay-availability-badge available" } else { "stay-availability-badge unavailable" }, if card.available { if card.category == "House" { "{card.available_rooms} rooms open" } else { "Available" } } else { "Unavailable for these dates" } }
                            }
                            div { class: "stay-card-body",
                                p { class: "stay-card-area", "{card.area}" }
                                a { class: "stay-card-title", href: "{card.href}", "{card.title}" }
                                if let Some(intersection) = card.nearby_intersection.as_ref() { p { class: "stay-card-address", "Near {intersection}" } }
                                div { class: "stay-card-footer",
                                    div { class: "stay-price",
                                        strong { "{price_label(card.price)}" } span { " / month" }
                                        small { if card.category == "House" { "Rooms from" } else { "Monthly rent" } }
                                    }
                                    if card.tours_available && card.available {
                                        a { class: "stay-tour-link", href: "{tour_url_from(&card, &stays_return)}", "Book a tour" }
                                    } else if !card.available {
                                        a { class: "stay-tour-link", href: "{card.href}", "View dates" }
                                    } else {
                                        span { class: "stay-tour-unavailable", "Tours unavailable" }
                                    }
                                }
                            }
                        }
                    }
                }
                if !loading() && !listings().is_empty() {
                }
            }
            footer { class: "stays-footer",
                div { class: "stays-brand", img { class: "brand-symbol", src: "/icons/buildry-mark.svg?v=20261017-vibrant", alt: "" } span { "Buildry" } }
                p { "Questions about a stay? We’re happy to help." }
                a { class: "stays-map-credit", href: "https://www.openstreetmap.org/copyright", "Map data © OpenStreetMap contributors" }
                a { class: "stays-privacy-link", href: "/privacy", "Privacy policy" }
                button { r#type: "button", onclick: move |_| contact_open.set(true), "Contact us" }
            }
            if contact_open() {
                div { class: "stay-modal-backdrop", role: "presentation", onclick: move |_| contact_open.set(false) }
                section { class: "stay-contact-modal", role: "dialog", aria_modal: "true", aria_label: "Contact options",
                    button { class: "stay-modal-close", r#type: "button", aria_label: "Close contact options", onclick: move |_| contact_open.set(false), Icon { name: "close" } }
                    p { class: "stays-eyebrow", "GET IN TOUCH" }
                    h2 { "Contact Buildry" }
                    p { "Our messaging details will be added here soon. You can book a house tour now." }
                    div { class: "stay-contact-options",
                        span { "WhatsApp", small { "Link coming soon" } }
                        span { "WeChat", small { "ID or QR code coming soon" } }
                        span { "Facebook Messenger", small { "Link coming soon" } }
                    }
                    a { class: "stay-contact-tour", href: "/book?return_to=%2Fstays", "Book a tour" }
                }
            }
        }
    }
}

pub fn guest_detail_page() -> Element {
    let (initial_year, initial_start, initial_months) = default_period();
    let path = web_sys::window()
        .and_then(|window| window.location().pathname().ok())
        .unwrap_or_default();
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
    let is_room = segments.get(1) == Some(&"room");
    let property_id = segments.get(2).and_then(|value| value.parse::<u32>().ok());
    let room_id = if is_room {
        segments.get(3).and_then(|value| value.parse::<u32>().ok())
    } else {
        None
    };
    let mut listings = use_signal(Vec::<GuestListing>::new);
    let mut rental_year = use_signal(|| initial_year);
    let mut rental_start = use_signal(|| initial_start);
    let mut rental_months = use_signal(|| initial_months);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut contact_open = use_signal(|| false);
    let mut gallery_open = use_signal(|| None::<usize>);

    use_effect(move || {
        spawn(async move {
            match Request::get("/api/guest-listings").send().await {
                Ok(response) if response.ok() => match response.json::<Vec<GuestListing>>().await {
                    Ok(result) => {
                        let rooms: Vec<_> = result
                            .iter()
                            .filter(|item| Some(item.id) == property_id)
                            .flat_map(|item| {
                                item.rooms
                                    .iter()
                                    .filter(|room| !is_room || Some(room.id) == room_id)
                                    .cloned()
                            })
                            .collect();
                        if let Some((year, start, months)) =
                            open_period(&rooms, rental_year(), &rental_start(), rental_months())
                        {
                            rental_year.set(year);
                            rental_start.set(start);
                            rental_months.set(months);
                        }
                        listings.set(result);
                    }
                    Err(_) => error.set("We couldn't load this stay. Please try again.".into()),
                },
                _ => error.set("We couldn't load this stay. Please try again.".into()),
            }
            loading.set(false);
        });
    });

    let listing = listings()
        .into_iter()
        .find(|item| Some(item.id) == property_id);
    let chosen_room = listing
        .as_ref()
        .and_then(|property| property.rooms.iter().find(|room| Some(room.id) == room_id))
        .cloned();
    let period = period_query(rental_year(), &rental_start(), rental_months());
    let detail_url = format!("{path}?{period}");
    let stays_url = format!("/stays?{period}");
    let property_cards = listing
        .as_ref()
        .map(|item| {
            cards_for(
                vec![item.clone()],
                rental_year(),
                &rental_start(),
                rental_months(),
            )
        })
        .unwrap_or_default();
    let house = property_cards
        .iter()
        .find(|card| card.category == "House")
        .cloned();
    let rooms: Vec<StayCard> = property_cards
        .into_iter()
        .filter(|card| card.category != "House")
        .collect();
    let current = if is_room {
        rooms
            .iter()
            .find(|card| card.href.split('?').next() == Some(path.as_str()) && room_id.is_some())
            .cloned()
    } else {
        house.clone()
    };
    let gallery_photos = current
        .as_ref()
        .map(|card| {
            if card.photos.is_empty() {
                vec![card.image.clone()]
            } else {
                card.photos.clone()
            }
        })
        .unwrap_or_default();
    use_effect(move || {
        if let Some(index) = gallery_open() {
            spawn(async move {
                TimeoutFuture::new(300).await;
                if let Some(element) = web_sys::window()
                    .and_then(|window| window.document())
                    .and_then(|document| {
                        document.get_element_by_id(&format!("stay-gallery-photo-{index}"))
                    })
                {
                    element.scroll_into_view();
                }
            });
        }
    });
    let mut other_rooms: Vec<StayCard> = rooms
        .into_iter()
        .filter(|card| !is_room || card.href.split('?').next() != Some(path.as_str()))
        .collect();
    other_rooms.sort_by_key(|card| !card.available);
    let room_count = if is_room {
        other_rooms.len() + 1
    } else {
        other_rooms.len()
    };
    let header_book_url = property_id
        .map(|id| {
            let room = room_id
                .filter(|id| *id > 0)
                .map(|id| format!("&room_id={id}"))
                .unwrap_or_default();
            format!(
                "/book?property_id={id}{room}&{period}&return_to={}",
                urlencoding::encode(&detail_url)
            )
        })
        .unwrap_or_else(|| {
            format!(
                "/book?{period}&return_to={}",
                urlencoding::encode(&stays_url)
            )
        });
    let amenities = if is_room {
        chosen_room
            .as_ref()
            .map(|room| room.amenities.clone())
            .unwrap_or_default()
    } else {
        listing
            .as_ref()
            .map(|item| item.amenities.clone())
            .unwrap_or_default()
    };
    let availability_end_year = period_keys(rental_year(), &rental_start(), rental_months())
        .and_then(|periods| {
            periods
                .last()
                .and_then(|period| period.split('-').next())
                .and_then(|year| year.parse::<i32>().ok())
        })
        .unwrap_or(rental_year())
        .max(rental_year() + 1);

    rsx! {
        document::Stylesheet { href: crate::CSS }
        main { class: "stays-page stay-detail-page",
            header { class: "stays-header",
                div { class: "stays-header-inner",
                    a { class: "stays-brand", href: "{stays_url}", img { class: "brand-symbol", src: "/icons/buildry-mark.svg?v=20261017-vibrant", alt: "" } span { "Buildry" } }
                    nav { aria_label: "Guest navigation",
                        a { href: "/login", "Sign in" }
                    }
                }
            }
            if loading() {
                p { class: "stays-state stay-detail-state", role: "status", "Loading stay…" }
            } else if !error().is_empty() {
                p { class: "stays-state stay-detail-state", role: "alert", "{error}" }
            } else if let Some(card) = current {
                document::Title { "{card.title} | Buildry" }
                div { class: "stay-detail-wrap",
                    nav { class: "stay-breadcrumbs", aria_label: "Breadcrumb",
                        a { href: "{stays_url}", "All stays" }
                        span { Icon { name: "chevron-right" } }
                        if is_room {
                            if let Some(house_card) = house.as_ref() {
                                a { href: "{house_card.href}", title: "{house_card.title}", "{breadcrumb_label(&house_card.title)}" }
                                span { Icon { name: "chevron-right" } }
                            }
                        }
                        strong { title: "{card.title}", "{breadcrumb_label(&card.title)}" }
                    }
                    section { class: "stay-detail-hero",
                        div { class: if gallery_photos.len() > 1 { if gallery_photos.len() == 2 { "stay-detail-visuals has-gallery two-photos" } else { "stay-detail-visuals has-gallery" } } else { "stay-detail-visuals" }, aria_label: "Photo gallery",
                            for (index, photo) in gallery_photos.iter().enumerate().take(3) {
                                button { key: "{index}", r#type: "button", class: if index == 0 { "stay-gallery-tile is-cover" } else { "stay-gallery-tile" }, aria_label: "View photo {index + 1} of {gallery_photos.len()}", onclick: move |_| gallery_open.set(Some(index)),
                                    img { src: "{photo}", alt: if index == 0 { if is_room { "Room photo" } else { "House photo" } } else { "Additional photo" } }
                                    if index == 0 {
                                        if is_ai_illustration(photo) { span { class: "stay-gallery-note", "AI-generated illustration" } }
                                        else if card.photos.is_empty() { span { class: "stay-gallery-note", "Illustrative photo" } }
                                    }
                                    if index == 2 && gallery_photos.len() > 3 { span { class: "stay-gallery-count", "View all {gallery_photos.len()} photos" } }
                                }
                            }
                        }
                        div { class: "stay-detail-summary",
                            p { class: "stays-eyebrow", "{card.category} · {card.area}" }
                            h1 { "{card.title}" }
                            if let Some(intersection) = card.nearby_intersection.as_ref() { p { class: "stay-detail-address", "Near {intersection}" } }
                            div { class: "stay-detail-facts",
                                if is_room {
                                    if let Some(room) = chosen_room.as_ref() {
                                        if !room.bathroom_access.is_empty() { span { "{room.bathroom_access} bathroom" } }
                                    }
                                    span { "Part of a {room_count}-room home" }
                                } else {
                                    span { "{room_count} rooms to explore" }
                                    span { "Waterloo, Ontario" }
                                }
                            }
                            div { class: "stay-detail-price",
                                if is_room { span { "Monthly rent for selected period" } } else { span { "Rooms from" } }
                                strong { "{price_label(card.price)}" } span { " / month" }
                            }
                        }
                    }
                    section { class: "stay-detail-experience",
                        aside { class: "stay-period-card",
                            p { class: "stays-eyebrow", "PLAN YOUR STAY" }
                            h2 { "Choose your rental period" }
                            RentalCalendar { rental_year, rental_start, rental_months, rooms: if is_room { chosen_room.clone().into_iter().collect() } else { listing.as_ref().map(|item| item.rooms.clone()).unwrap_or_default() }, compact: false }
                            p { class: if card.available { "stay-period-status available" } else { "stay-period-status unavailable" },
                                if card.available { if is_room { "This room is open for your selected period." } else { "{card.available_rooms} rooms are open for this period." } }
                                else { "Unavailable for part of this rental period." }
                            }
                            if card.tours_available && card.available { a { class: "stay-period-cta", href: "{tour_url(&card)}", "Book a tour" } }
                        }
                    }
                    section { class: "stay-amenities",
                        div { class: "stay-detail-section-head", h2 { "What this place offers" } }
                        if amenities.is_empty() { p { "Room and house features are listed below." } }
                        else { div { class: "stay-amenity-grid", for amenity in &amenities { div { class: "stay-amenity", span { Icon { name: "check" } } strong { "{amenity}" } } } } }
                        if is_room {
                            if let Some(item) = listing.as_ref() {
                                if !item.amenities.is_empty() { p { class: "stay-shared-label", "In the house" } div { class: "stay-amenity-grid", for amenity in &item.amenities { div { class: "stay-amenity", span { Icon { name: "house" } } strong { "{amenity}" } } } } }
                            }
                        }
                    }
                    section { class: "stay-availability-overview",
                        div { class: "stay-detail-section-head", h2 { "Availability at a glance" } }
                        for display_year in rental_year()..=availability_end_year {
                            div { class: "stay-availability-year",
                                h3 { "{display_year}" }
                                div { class: "stay-availability-tiles",
                                    for (season_key, season_label) in [("jan", "Jan–Apr"), ("may", "May–Aug"), ("sep", "Sep–Dec")] {
                                        {
                                            let open_rooms = listing.as_ref().map(|item| item.rooms.iter().filter(|room| room.is_available(display_year, season_key, 4)).count()).unwrap_or(0);
                                            let open = if is_room { chosen_room.as_ref().is_some_and(|room| room.is_available(display_year, season_key, 4)) } else { open_rooms > 0 };
                                            rsx! { div { class: if open { "stay-availability-tile open" } else { "stay-availability-tile closed" },
                                                strong { "{season_label}" }
                                                span { if open { if is_room { "Available" } else { "{open_rooms} rooms open" } } else { "Unavailable" } }
                                            } }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if is_room {
                        if let Some(house_card) = house.as_ref() {
                            a { class: "stay-house-context", href: "{house_card.href}", aria_label: "View {house_card.title}",
                                img { src: "{house_card.image}", alt: "Illustrative house exterior" }
                                div {
                                    p { class: "stays-eyebrow", "THE HOUSE" }
                                    h2 { "{house_card.title}" }
                                    p { "See the house and all of its rooms." }
                                    span { class: "stay-house-context-action", "View house" }
                                }
                            }
                        }
                    }
                    section { class: "stay-detail-rooms",
                        div { class: "stay-detail-section-head",
                                h2 { if is_room { "Other rooms in this house" } else { "Rooms in this house" } }
                            if is_room {
                                if let Some(house_card) = house.as_ref() { a { href: "{house_card.href}", "View house" } }
                            }
                        }
                        if other_rooms.is_empty() {
                            p { class: "stays-state", "No other rooms are listed for this house yet." }
                        } else {
                            div { class: "stays-grid",
                                for room in other_rooms {
                                    article { key: "{room.key}", class: if room.available { "stay-card" } else { "stay-card stay-card-unavailable" },
                                        a { class: "stay-card-image", href: "{room.href}", aria_label: "View {room.title} in {room.area}",
                                            img { src: "{room.image}", alt: "Illustrative room interior" }
                                            span { class: "stay-card-type", "{room.category}" }
                                            if is_ai_illustration(&room.image) { span { class: "stay-card-illustrative", "AI illustration" } }
                                            span { class: if room.available { "stay-availability-badge available" } else { "stay-availability-badge unavailable" }, if room.available { "Available" } else { "Unavailable for these dates" } }
                                        }
                                        div { class: "stay-card-body",
                                            p { class: "stay-card-area", "{room.area}" }
                                            a { class: "stay-card-title", href: "{room.href}", "{room.title}" }
                                            if let Some(intersection) = room.nearby_intersection.as_ref() { p { class: "stay-card-address", "Near {intersection}" } }
                                            div { class: "stay-card-footer",
                                                div { class: "stay-price",
                                                    strong { "{price_label(room.price)}" } span { " / month" }
                                                    small { "Monthly rent" }
                                                }
                                                a { class: "stay-tour-link", href: "{room.href}", "View room" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                document::Title { "Stay not found | Buildry" }
                section { class: "stay-detail-not-found",
                    h1 { "Stay not found" }
                    p { "This listing may have changed or been removed." }
                    a { href: "/stays", "Explore stays" }
                }
            }
            footer { class: "stays-footer",
                div { class: "stays-brand", img { class: "brand-symbol", src: "/icons/buildry-mark.svg?v=20261017-vibrant", alt: "" } span { "Buildry" } }
                p { "Questions about a stay? We’re happy to help." }
                a { class: "stays-map-credit", href: "https://www.openstreetmap.org/copyright", "Map data © OpenStreetMap contributors" }
                a { class: "stays-privacy-link", href: "/privacy", "Privacy policy" }
                button { r#type: "button", onclick: move |_| contact_open.set(true), "Contact us" }
            }
            if gallery_open().is_some() && !gallery_photos.is_empty() {
                div { class: "gallery-viewer stay-gallery-viewer", role: "dialog", aria_modal: "true", aria_label: "Stay photos",
                    div { class: "gallery-viewer-top",
                        button { r#type: "button", class: "gallery-back back-icon-button", aria_label: "Back to stay details", onclick: move |_| gallery_open.set(None), Icon { name: "arrow-left" } }
                        span { "{gallery_photos.len()} photos" }
                    }
                    div { class: "gallery-viewer-scroll",
                        div { class: "gallery-viewer-stack",
                            for (index, photo) in gallery_photos.iter().enumerate() {
                                figure { id: "stay-gallery-photo-{index}", class: "gallery-viewer-photo",
                                    img { src: "{photo}", alt: "Photo {index + 1} of {gallery_photos.len()}" }
                                    figcaption { "{index + 1} of {gallery_photos.len()}" }
                                }
                            }
                        }
                    }
                }
            }
            if contact_open() {
                div { class: "stay-modal-backdrop", role: "presentation", onclick: move |_| contact_open.set(false) }
                section { class: "stay-contact-modal", role: "dialog", aria_modal: "true", aria_label: "Contact options",
                    button { class: "stay-modal-close", r#type: "button", aria_label: "Close contact options", onclick: move |_| contact_open.set(false), Icon { name: "close" } }
                    p { class: "stays-eyebrow", "GET IN TOUCH" }
                    h2 { "Contact Buildry" }
                    p { "Our messaging details will be added here soon. You can book a house tour now." }
                    div { class: "stay-contact-options",
                        span { "WhatsApp", small { "Link coming soon" } }
                        span { "WeChat", small { "ID or QR code coming soon" } }
                        span { "Facebook Messenger", small { "Link coming soon" } }
                    }
                    a { class: "stay-contact-tour", href: "{header_book_url}", "Book a tour" }
                }
            }
        }
    }
}
