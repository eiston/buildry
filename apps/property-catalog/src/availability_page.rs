use crate::{api, house_badge::HouseBadge, model::Property};
use dioxus::prelude::*;

const SEASONS: [(&str, &str); 3] = [("jan", "Jan–Apr"), ("may", "May–Aug"), ("sep", "Sep–Dec")];

pub fn availability_page() -> Element {
    let current_year = js_sys::Date::new_0().get_full_year() as i32;
    let mut properties = use_signal(Vec::<Property>::new);
    let mut year = use_signal(|| current_year + 1);
    let mut search = use_signal(String::new);
    let mut loading = use_signal(|| true);
    let mut saving = use_signal(|| false);
    let mut message = use_signal(String::new);
    use_effect(move || {
        spawn(async move {
            match api::list().await {
                Ok(data) => properties.set(data),
                Err(error) => message.set(error),
            }
            loading.set(false);
        });
    });

    let query = search().to_lowercase();
    let all = properties();
    let total_rooms: usize = all
        .iter()
        .map(|house| {
            house
                .spaces
                .iter()
                .filter(|room| room.kind == "Bedroom" || room.kind == "Suite")
                .count()
        })
        .sum();
    let visible: Vec<Property> = all
        .into_iter()
        .filter(|house| {
            query.is_empty()
                || house.id.to_string().contains(&query)
                || house.name.to_lowercase().contains(&query)
                || house.address.to_lowercase().contains(&query)
                || house
                    .spaces
                    .iter()
                    .any(|room| room.name.to_lowercase().contains(&query))
        })
        .collect();
    rsx! {
        document::Title { "Availability | Buildry" }
        document::Stylesheet { href: crate::CSS }
        div { class: "app-shell",
            aside { class: "sidebar",
                div { class: "sidebar-header", div { class: "brand", span { class: "brand-mark", "B" } span { class: "brand-name", "buildry" } } }
                a { class: "nav-item tour-nav-link", href: "/", title: "Properties", span { class: "nav-icon icon-properties" } span { class: "nav-text", "Properties" } }
                a { class: "nav-item tour-nav-link", href: "/pricing", title: "Pricing", span { class: "nav-icon icon-pricing" } span { class: "nav-text", "Pricing" } }
                a { class: "nav-item tour-nav-link", href: "/tours", title: "House tours", span { class: "nav-icon icon-tours" } span { class: "nav-text", "House tours" } }
                a { class: "nav-item tour-nav-link active", href: "/availability", title: "Availability", span { class: "nav-icon icon-availability" } span { class: "nav-text", "Availability" } }
                a { class: "nav-item tour-nav-link", href: "/stays", title: "Guest stays", span { class: "nav-icon icon-stays" } span { class: "nav-text", "Guest stays" } }
                form { class: "nav-logout", method: "post", action: "/logout", button { class: "nav-item tour-nav-link", r#type: "submit", title: "Sign out", span { class: "nav-icon icon-logout" } span { class: "nav-text", "Sign out" } } }
            }
            main { class: "main-content availability-admin availability-overview",
                section { class: "page-head", div { h1 { "Availability" } p { class: "catalog-summary", "{visible.len()} houses · {total_rooms} rooms and suites" } } }
                if loading() { p { class: "stays-state", "Loading availability…" } }
                if !message().is_empty() { p { class: "availability-message", role: "status", "{message}" } }
                div { class: "availability-overview-toolbar",
                    input { class: "search-input", aria_label: "Search houses and rooms", placeholder: "Search house, address or room", value: "{search}", oninput: move |event| search.set(event.value()) }
                    label { "Year", select { value: "{year}", onchange: move |event| year.set(event.value().parse().unwrap_or(current_year + 1)),
                        for choice_year in current_year..=current_year + 7 { option { value: "{choice_year}", selected: year() == choice_year, "{choice_year}" } }
                    } }
                }
                if visible.is_empty() && !loading() { p { class: "stays-state", "No houses match your search." } }
                for house in visible {
                    section { key: "{house.id}", class: "availability-house",
                        div { class: "availability-house-head",
                            div { HouseBadge { id: house.id } div { h2 { "{house.name}" } if house.name != house.address { p { "{house.address}" } } } }
                            a { href: "/properties/{house.id}", "Manage house →" }
                        }
                        div { class: "availability-matrix-scroll",
                            div { class: "availability-matrix",
                                div { class: "availability-matrix-header", span { "Room / suite" } for (_, label) in SEASONS { span { "{label} {year}" } } }
                                for room in house.spaces.iter().filter(|room| room.kind == "Bedroom" || room.kind == "Suite") {
                                    div { key: "{room.id}", class: "availability-matrix-row",
                                        div { class: "availability-matrix-room", strong { "{room.name}" } small { if room.kind == "Suite" { "Suite" } else { "Room" } } }
                                        for (season_key, _) in SEASONS {
                                            {
                                                let period = format!("{}-{}", year(), season_key);
                                                let unavailable = room.unavailable_periods.contains(&period);
                                                let label = format!("{} {}: {}", room.name, period, if unavailable { "Unavailable" } else { "Available" });
                                                rsx! { button { key: "{period}", r#type: "button", class: if unavailable { "availability-cell unavailable" } else { "availability-cell available" }, disabled: saving(), aria_label: "{label}",
                                                    onclick: { let house_id = house.id; let room_id = room.id; let period = period.clone(); move |_| {
                                                        let Some(mut current) = properties().into_iter().find(|property| property.id == house_id) else { return; };
                                                        if let Some(space) = current.spaces.iter_mut().find(|space| space.id == room_id) {
                                                            if space.unavailable_periods.contains(&period) { space.unavailable_periods.retain(|value| value != &period); }
                                                            else { space.unavailable_periods.push(period.clone()); space.unavailable_periods.sort(); }
                                                        }
                                                        saving.set(true); message.set(String::new());
                                                        spawn(async move {
                                                            match api::save(&current).await {
                                                                Ok(saved) => { let mut list = properties(); if let Some(item) = list.iter_mut().find(|item| item.id == saved.id) { *item = saved; } properties.set(list); }
                                                                Err(error) => message.set(error),
                                                            }
                                                            saving.set(false);
                                                        });
                                                    } }, if unavailable { "Unavailable" } else { "Available" } } }
                                            }
                                        }
                                    }
                                }
                                if house.spaces.iter().all(|room| room.kind != "Bedroom" && room.kind != "Suite") { p { class: "empty", "No rooms or suites yet." } }
                            }
                        }
                    }
                }
            }
        }
    }
}
