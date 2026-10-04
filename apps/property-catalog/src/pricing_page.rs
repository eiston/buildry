use crate::{
    api,
    model::{price_for_month, starter_price_curve, PricePoint, Property, Space},
    CSS,
};
use dioxus::prelude::*;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn edit_room(
    mut properties: Signal<Vec<Property>>,
    mut dirty: Signal<Vec<u32>>,
    house_id: u32,
    room_id: u32,
    change: impl FnOnce(&mut Space),
) {
    let mut all = properties.write();
    if let Some(room) = all
        .iter_mut()
        .find(|house| house.id == house_id)
        .and_then(|house| house.spaces.iter_mut().find(|room| room.id == room_id))
    {
        change(room);
    }
    if !dirty().contains(&house_id) {
        dirty.write().push(house_id);
    }
}

pub fn pricing_page() -> Element {
    let mut properties = use_signal(Vec::<Property>::new);
    let mut selected = use_signal(|| None::<(u32, u32)>);
    let mut search = use_signal(String::new);
    let mut preview_month = use_signal(|| 9_u8);
    let mut dirty = use_signal(Vec::<u32>::new);
    let mut saving = use_signal(|| false);
    let mut loading = use_signal(|| true);
    let mut message = use_signal(String::new);
    use_effect(move || {
        spawn(async move {
            match api::list().await {
                Ok(data) => {
                    selected.set(
                        data.iter()
                            .flat_map(|house| {
                                house
                                    .spaces
                                    .iter()
                                    .filter(|room| room.kind == "Bedroom" || room.kind == "Suite")
                                    .map(move |room| (house.id, room.id))
                            })
                            .next(),
                    );
                    properties.set(data);
                }
                Err(error) => message.set(error),
            }
            loading.set(false);
        });
    });
    let query = search().to_lowercase();
    let all = properties();
    let selected_room = selected().and_then(|(house_id, room_id)| {
        all.iter()
            .find(|house| house.id == house_id)
            .and_then(|house| {
                house
                    .spaces
                    .iter()
                    .find(|room| room.id == room_id)
                    .map(|room| (house.clone(), room.clone()))
            })
    });
    let room_count: usize = all
        .iter()
        .map(|house| {
            house
                .spaces
                .iter()
                .filter(|room| room.kind == "Bedroom" || room.kind == "Suite")
                .count()
        })
        .sum();
    rsx! {
        document::Title { "Pricing | Buildry" }
        document::Stylesheet { href: CSS }
        div { class: "app-shell",
            aside { class: "sidebar",
                div { class: "sidebar-header", div { class: "brand", span { class: "brand-mark", "B" } span { class: "brand-name", "buildry" } } }
                a { class: "nav-item tour-nav-link", href: "/", title: "Properties", span { class: "nav-icon icon-properties" } span { class: "nav-text", "Properties" } }
                a { class: "nav-item tour-nav-link active", href: "/pricing", title: "Pricing", span { class: "nav-icon icon-pricing" } span { class: "nav-text", "Pricing" } }
                a { class: "nav-item tour-nav-link", href: "/tours", title: "House tours", span { class: "nav-icon icon-tours" } span { class: "nav-text", "House tours" } }
                a { class: "nav-item tour-nav-link", href: "/availability", title: "Availability", span { class: "nav-icon icon-availability" } span { class: "nav-text", "Availability" } }
                a { class: "nav-item tour-nav-link", href: "/stays", title: "Guest stays", span { class: "nav-icon icon-stays" } span { class: "nav-text", "Guest stays" } }
                form { class: "nav-logout", method: "post", action: "/logout", button { class: "nav-item tour-nav-link", r#type: "submit", title: "Sign out", span { class: "nav-icon icon-logout" } span { class: "nav-text", "Sign out" } } }
            }
            main { class: "main-content pricing-admin",
                section { class: "page-head",
                    div { h1 { "Pricing" } p { class: "catalog-summary", "{all.len()} houses · {room_count} rooms and suites" } }
                    button { class: "primary-button", disabled: saving() || dirty().is_empty(), onclick: move |_| {
                        let updates: Vec<Property> = properties().into_iter().filter(|house| dirty().contains(&house.id)).collect();
                        saving.set(true); message.set(String::new());
                        spawn(async move {
                            for house in updates {
                                match api::save(&house).await {
                                    Ok(saved) => { let mut list = properties(); if let Some(item) = list.iter_mut().find(|item| item.id == saved.id) { *item = saved; } properties.set(list); dirty.write().retain(|id| *id != house.id); }
                                    Err(error) => { message.set(error); saving.set(false); return; }
                                }
                            }
                            message.set("Prices saved.".into()); saving.set(false);
                        });
                    }, if saving() { "Saving…" } else { "Save prices" } }
                }
                if loading() { p { class: "stays-state", "Loading prices…" } }
                if !message().is_empty() { p { class: "availability-message", role: "status", "{message}" } }
                div { class: "pricing-layout",
                    section { class: "pricing-list",
                        input { class: "search-input", aria_label: "Search houses and rooms", placeholder: "Search house or room", value: "{search}", oninput: move |event| search.set(event.value()) }
                        for house in all.iter().filter(|house| query.is_empty() || house.id.to_string().contains(&query) || house.name.to_lowercase().contains(&query) || house.address.to_lowercase().contains(&query) || house.spaces.iter().any(|room| room.name.to_lowercase().contains(&query))) {
                            div { key: "{house.id}", class: "pricing-house-group",
                                div { class: "pricing-house-title", span { class: "house-number", "{house.id}" } div { strong { "{house.name}" } small { "{house.address}" } } }
                                for room in house.spaces.iter().filter(|room| (room.kind == "Bedroom" || room.kind == "Suite") && (query.is_empty() || house.name.to_lowercase().contains(&query) || house.address.to_lowercase().contains(&query) || room.name.to_lowercase().contains(&query) || house.id.to_string().contains(&query))) {
                                    button { key: "{room.id}", class: if selected() == Some((house.id, room.id)) { "pricing-room-row selected" } else { "pricing-room-row" }, onclick: { let house_id = house.id; let room_id = room.id; move |_| selected.set(Some((house_id, room_id))) },
                                        span { "{room.name}" }
                                        strong { if let Some(base) = room.current_monthly_rent.or(room.rent_sep_dec).or(room.rent_jan_apr).or(room.rent_may_aug) { "${base}" } else { "Set base" } }
                                    }
                                }
                            }
                        }
                    }
                    if let Some((house, room)) = selected_room {
                        section { class: "pricing-editor",
                            div { class: "pricing-editor-head", div { p { class: "eyebrow", "House {house.id} · {house.address}" } h2 { "{room.name}" } } a { href: "/properties/{house.id}", "Manage property →" } }
                            div { class: "pricing-base-card", label { "Base monthly rent", div { class: "pricing-money-input", span { "$" } input { r#type: "number", min: "1", value: room.current_monthly_rent.or(room.rent_sep_dec).or(room.rent_jan_apr).or(room.rent_may_aug).map(|v| v.to_string()).unwrap_or_default(), oninput: { let house_id = house.id; let room_id = room.id; move |event| edit_room(properties, dirty, house_id, room_id, |room| room.current_monthly_rent = event.value().parse().ok()) } } } } }
                            {
                                let curve = if room.price_curve.is_empty() { starter_price_curve() } else { room.price_curve.clone() };
                                let base = room.current_monthly_rent.or(room.rent_sep_dec).or(room.rent_jan_apr).or(room.rent_may_aug).unwrap_or(0);
                                let quote = price_for_month(base, &curve, preview_month());
                                let points = {
                                    let mut sorted = curve.clone(); sorted.sort_by_key(|point| point.month);
                                    sorted.iter().map(|point| format!("{},{}", 12 + (point.month as u32 - 1) * 476 / 11, 154_u32.saturating_sub(point.percent.min(200) * 140 / 200))).collect::<Vec<_>>().join(" ")
                                };
                                rsx! {
                                    div { class: "pricing-preview",
                                        div { strong { "${quote} / month" } span { "{MONTHS[(preview_month() - 1) as usize]}" } }
                                        input { r#type: "range", min: "1", max: "12", value: "{preview_month}", oninput: move |event| preview_month.set(event.value().parse().unwrap_or(9)) }
                                    }
                                    svg { class: "pricing-curve-chart", view_box: "0 0 500 170", role: "img", line { x1: "12", y1: "84", x2: "488", y2: "84" } polyline { points: "{points}" } }
                                    div { class: "pricing-month-axis", for label in MONTHS { span { "{label}" } } }
                                    div { class: "pricing-points-head", h3 { "Annual price curve" } button { class: "text-button", onclick: { let house_id = house.id; let room_id = room.id; move |_| edit_room(properties, dirty, house_id, room_id, |room| { if room.price_curve.is_empty() { room.price_curve = starter_price_curve(); } if room.price_curve.len() < 12 { let next = (1..=12).find(|month| !room.price_curve.iter().any(|point| point.month == *month)).unwrap_or(1); room.price_curve.push(PricePoint { month: next, percent: 100 }); } }) }, "+ Add point" } }
                                    div { class: "pricing-points-table",
                                        div { class: "pricing-point labels", span { "Month" } span { "% of base" } span { "Monthly price" } span {} }
                                        for (index, point) in curve.iter().enumerate() {
                                            div { key: "{index}", class: "pricing-point",
                                                select { value: "{point.month}", onchange: { let house_id = house.id; let room_id = room.id; move |event| { if let Ok(value) = event.value().parse::<u8>() { edit_room(properties, dirty, house_id, room_id, |room| { if room.price_curve.is_empty() { room.price_curve = starter_price_curve(); } if let Some(point) = room.price_curve.get_mut(index) { point.month = value; } }); } } }, for (month_index, label) in MONTHS.iter().enumerate() { option { value: "{month_index + 1}", disabled: curve.iter().enumerate().any(|(other_index, other)| other_index != index && other.month == (month_index + 1) as u8), "{label}" } } }
                                                input { r#type: "number", min: "1", max: "500", value: "{point.percent}", oninput: { let house_id = house.id; let room_id = room.id; move |event| { if let Ok(value) = event.value().parse::<u32>() { edit_room(properties, dirty, house_id, room_id, |room| { if room.price_curve.is_empty() { room.price_curve = starter_price_curve(); } if let Some(point) = room.price_curve.get_mut(index) { point.percent = value; } }); } } } }
                                                strong { "${(base as u64 * point.percent as u64 / 100) as u32}" }
                                                button { class: "text-button danger", aria_label: "Remove price point", disabled: curve.len() <= 2, onclick: { let house_id = house.id; let room_id = room.id; move |_| edit_room(properties, dirty, house_id, room_id, |room| { if room.price_curve.is_empty() { room.price_curve = starter_price_curve(); } if room.price_curve.len() > 2 { room.price_curve.remove(index); } }) }, "×" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else if !loading() { section { class: "pricing-editor", p { class: "empty", "Choose a room or suite." } } }
                }
            }
        }
    }
}
