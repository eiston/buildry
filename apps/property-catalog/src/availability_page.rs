use crate::{api, model::Property};
use dioxus::prelude::*;

const SEASONS: [(&str, &str); 3] = [("jan", "Jan–Apr"), ("may", "May–Aug"), ("sep", "Sep–Dec")];

pub fn availability_page() -> Element {
    let current_year = js_sys::Date::new_0().get_full_year() as i32;
    let mut properties = use_signal(Vec::<Property>::new);
    let mut selected_id = use_signal(|| 0_u32);
    let mut year = use_signal(|| current_year + 1);
    let mut loading = use_signal(|| true);
    let mut saving = use_signal(String::new);
    let mut message = use_signal(String::new);

    use_effect(move || {
        spawn(async move {
            match api::list().await {
                Ok(data) => {
                    if let Some(first) = data.first() {
                        selected_id.set(first.id);
                    }
                    properties.set(data);
                }
                Err(error) => message.set(error),
            }
            loading.set(false);
        });
    });

    let selected = properties()
        .into_iter()
        .find(|property| property.id == selected_id());
    let rooms = selected
        .as_ref()
        .map(|property| {
            property
                .spaces
                .iter()
                .filter(|space| space.kind == "Bedroom" || space.kind == "Suite")
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    rsx! {
        document::Title { "Room availability | Buildry" }
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
            main { class: "main-content availability-admin",
                section { class: "page-head", div { h1 { "Room availability" } p { class: "catalog-summary", "Manage each four-month rental block. Longer stays need every block to be open." } } }
                if loading() { p { class: "stays-state", "Loading rooms…" } }
                if !message().is_empty() { p { class: "availability-message", role: "status", "{message}" } }
                div { class: "availability-toolbar",
                    label { "House", select { value: "{selected_id}", onchange: move |event| selected_id.set(event.value().parse().unwrap_or(0)),
                        for property in properties() { option { key: "{property.id}", value: "{property.id}", selected: selected_id() == property.id, "House {property.id} · {property.name} · {property.address}" } }
                    } }
                    label { "Year", select { value: "{year}", onchange: move |event| year.set(event.value().parse().unwrap_or(current_year + 1)),
                        for choice_year in current_year..=current_year + 7 { option { value: "{choice_year}", selected: year() == choice_year, "{choice_year}" } }
                    } }
                }
                p { class: "availability-help", "Tap a season to switch between available and unavailable. Example data is identified until you verify each room." }
                if rooms.is_empty() && !loading() { p { class: "stays-state", "No rooms are recorded for this house." } }
                div { class: "availability-rooms",
                    for room in rooms {
                        article { key: "{room.id}", class: "availability-room",
                            div { class: "availability-room-head",
                                div { h2 { "{room.name}" } p { if room.availability_confirmed { "Availability verified" } else { "Example availability · review before confirming" } } }
                                button { r#type: "button", class: "availability-verify", disabled: !saving().is_empty() || room.availability_confirmed, onclick: { let room_id = room.id; move |_| {
                                    let mut house = match properties().into_iter().find(|item| item.id == selected_id()) { Some(item) => item, None => return };
                                    if let Some(item) = house.spaces.iter_mut().find(|item| item.id == room_id) { item.availability_confirmed = true; }
                                    saving.set(format!("verify-{room_id}"));
                                    spawn(async move {
                                        match api::save(&house).await {
                                            Ok(saved) => { let mut all = properties(); if let Some(item) = all.iter_mut().find(|item| item.id == saved.id) { *item = saved; } properties.set(all); message.set("Availability verified.".into()); }
                                            Err(error) => message.set(error),
                                        }
                                        saving.set(String::new());
                                    });
                                } }, if room.availability_confirmed { "Verified" } else { "Mark verified" } }
                            }
                            div { class: "availability-season-grid",
                                for (season_key, label) in SEASONS {
                                    {
                                        let period = format!("{}-{}", year(), season_key);
                                        let unavailable = room.unavailable_periods.contains(&period);
                                        rsx! { button { key: "{period}", r#type: "button", class: if unavailable { "availability-season unavailable" } else { "availability-season available" }, disabled: !saving().is_empty(), onclick: { let room_id = room.id; let period = period.clone(); move |_| {
                                            let mut house = match properties().into_iter().find(|item| item.id == selected_id()) { Some(item) => item, None => return };
                                            if let Some(item) = house.spaces.iter_mut().find(|item| item.id == room_id) {
                                                if item.unavailable_periods.contains(&period) { item.unavailable_periods.retain(|value| value != &period); }
                                                else { item.unavailable_periods.push(period.clone()); item.unavailable_periods.sort(); }
                                                item.availability_confirmed = false;
                                            }
                                            saving.set(format!("{room_id}-{period}"));
                                            spawn(async move {
                                                match api::save(&house).await {
                                                    Ok(saved) => { let mut all = properties(); if let Some(item) = all.iter_mut().find(|item| item.id == saved.id) { *item = saved; } properties.set(all); message.set("Availability saved. Verify the room when all periods are correct.".into()); }
                                                    Err(error) => message.set(error),
                                                }
                                                saving.set(String::new());
                                            });
                                        } }, span { "{label} {year}" } strong { if unavailable { "Unavailable" } else { "Available" } } } }
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
