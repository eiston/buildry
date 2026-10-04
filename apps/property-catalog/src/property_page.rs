use crate::{
    api, map,
    model::{Property, Space},
    photo_manager::PhotoManager,
    CSS,
};
use dioxus::prelude::*;

fn edit_property(mut draft: Signal<Option<Property>>, change: impl FnOnce(&mut Property)) {
    if let Some(property) = draft.write().as_mut() {
        change(property);
    }
}

fn edit_space(draft: Signal<Option<Property>>, id: u32, change: impl FnOnce(&mut Space)) {
    edit_property(draft, |property| {
        if let Some(space) = property.spaces.iter_mut().find(|space| space.id == id) {
            change(space);
        }
    });
}

pub fn property_page() -> Element {
    let path = web_sys::window()
        .and_then(|window| window.location().pathname().ok())
        .unwrap_or_default();
    let requested_id = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .parse::<u32>()
        .ok();
    let is_new = path.ends_with("/new");
    let mut draft = use_signal(|| None::<Property>);
    let mut editing_space = use_signal(|| None::<u32>);
    let mut loading = use_signal(|| true);
    let mut saving = use_signal(|| false);
    let uploading = use_signal(|| false);
    let mut locating = use_signal(|| false);
    let mut message = use_signal(String::new);

    use_effect(move || {
        spawn(async move {
            match api::list().await {
                Ok(properties) => {
                    let selected = if is_new {
                        Some(Property::new(
                            properties.iter().map(|item| item.id).max().unwrap_or(0) + 1,
                        ))
                    } else {
                        properties
                            .into_iter()
                            .find(|item| Some(item.id) == requested_id)
                    };
                    if selected.is_none() {
                        message.set("Property not found.".into());
                    }
                    editing_space.set(
                        selected
                            .as_ref()
                            .and_then(|property| {
                                property
                                    .spaces
                                    .iter()
                                    .find(|space| space.kind == "Bedroom" || space.kind == "Suite")
                            })
                            .map(|space| space.id),
                    );
                    draft.set(selected);
                }
                Err(error) => message.set(error),
            }
            loading.set(false);
        });
    });

    let property = draft();
    let selected_space = property.as_ref().and_then(|item| {
        item.spaces
            .iter()
            .find(|space| {
                Some(space.id) == editing_space()
                    && (space.kind == "Bedroom" || space.kind == "Suite")
            })
            .cloned()
    });
    rsx! {
        document::Title { "Manage property | Buildry" }
        document::Stylesheet { href: CSS }
        div { class: "app-shell",
            aside { class: "sidebar",
                div { class: "sidebar-header", div { class: "brand", span { class: "brand-mark", "B" } span { class: "brand-name", "buildry" } } }
                a { class: "nav-item active", href: "/", title: "Properties", span { class: "nav-icon icon-properties" } span { class: "nav-text", "Properties" } }
                a { class: "nav-item tour-nav-link", href: "/pricing", title: "Pricing", span { class: "nav-icon icon-pricing" } span { class: "nav-text", "Pricing" } }
                a { class: "nav-item tour-nav-link", href: "/tours", title: "House tours", span { class: "nav-icon icon-tours" } span { class: "nav-text", "House tours" } }
                a { class: "nav-item tour-nav-link", href: "/availability", title: "Availability", span { class: "nav-icon icon-availability" } span { class: "nav-text", "Availability" } }
                a { class: "nav-item tour-nav-link", href: "/stays", title: "Guest stays", span { class: "nav-icon icon-stays" } span { class: "nav-text", "Guest stays" } }
                form { class: "nav-logout", method: "post", action: "/logout", button { class: "nav-item tour-nav-link", r#type: "submit", title: "Sign out", span { class: "nav-icon icon-logout" } span { class: "nav-text", "Sign out" } } }
            }
            main { class: "main-content property-admin",
                a { class: "back-link", href: "/", "← All properties" }
                if loading() { p { class: "stays-state", "Loading property…" } }
                if let Some(item) = property {
                    div { class: "page-head",
                        div { p { class: "eyebrow", "House {item.id}" } h1 { if is_new { "Add property" } else { "{item.name}" } } p { class: "catalog-summary", if item.address.trim().is_empty() { "Address not set" } else { "{item.address}" } } }
                        button { class: "primary-button", disabled: saving() || uploading(), onclick: move |_| {
                            if saving() || uploading() { return; }
                            let Some(mut current) = draft() else { return; };
                            current.name = current.name.trim().to_string();
                            current.address = current.address.trim().to_string();
                            if current.name.is_empty() || current.address.is_empty() { message.set("Enter a property name and street address.".into()); return; }
                            if let Some(space) = current.spaces.iter().find(|space| space.name.trim().is_empty()) { message.set(format!("Name every space before saving (space {}).", space.id)); return; }
                            saving.set(true);
                            message.set("Saving property…".into());
                            spawn(async move {
                                match api::save(&current).await {
                                    Ok(saved) => {
                                        let id = saved.id;
                                        draft.set(Some(saved));
                                        message.set("Saved in PostgreSQL.".into());
                                        if is_new { if let Some(window) = web_sys::window() { let _ = window.location().set_href(&format!("/properties/{id}")); } }
                                    }
                                    Err(error) => message.set(error),
                                }
                                saving.set(false);
                            });
                        }, if saving() { "Saving…" } else if uploading() { "Uploading…" } else { "Save property" } }
                    }
                    if !message().is_empty() { p { class: "availability-message", role: "status", "{message}" } }
                    div { class: "management-grid",
                        section { class: "management-card",
                            h2 { "House details" }
                            div { class: "management-fields",
                                label { "Property name", input { value: item.name.clone(), oninput: move |e| edit_property(draft, |p| p.name = e.value()) } }
                                label { "Street address", input { value: item.address.clone(), oninput: move |e| edit_property(draft, |p| { p.address = e.value(); p.latitude = None; p.longitude = None; p.map_label = None; }) } }
                                label { "Neighbourhood", input { value: item.area.clone(), oninput: move |e| edit_property(draft, |p| p.area = e.value()) } }
                                label { "Amenities", textarea { rows: "2", value: item.amenities.join(", "), oninput: move |e| edit_property(draft, |p| p.amenities = e.value().split(',').map(str::trim).filter(|x| !x.is_empty()).map(str::to_string).collect()) } }
                                label { "Notes", textarea { rows: "4", value: if item.notes.starts_with("Room breakdown entered from the supplied report.") || item.notes.starts_with("Address supplied.") { String::new() } else { item.notes.clone() }, oninput: move |e| edit_property(draft, |p| p.notes = e.value()) } }
                            }
                            details { class: "location-management room-extra",
                                summary { "Map location" }
                                if let (Some(latitude), Some(longitude)) = (item.latitude, item.longitude) { p { class: "management-help", "Pin: {latitude:.6}, {longitude:.6}" } }
                                else { p { class: "management-help", "No map pin saved for this address." } }
                                button { class: "outline-button", disabled: locating() || item.address.trim().is_empty(), onclick: move |_| {
                                    let Some(address) = draft().map(|p| p.address) else { return; };
                                    locating.set(true);
                                    spawn(async move {
                                        match map::locate(&address).await {
                                            Ok(found) => {
                                                edit_property(draft, |p| {
                                                    if p.address == address { p.latitude = Some(found.latitude); p.longitude = Some(found.longitude); p.map_label = Some(found.display_name); }
                                                });
                                                message.set("Location found. Save the property to keep the pin.".into());
                                            }
                                            Err(error) => message.set(error),
                                        }
                                        locating.set(false);
                                    });
                                }, if locating() { "Locating…" } else { "Locate address" } }
                            }
                        }
                        section { class: "management-card",
                            h2 { "House photos" }
                            PhotoManager { draft, room_id: None, uploading, message }
                        }
                    }
                    section { class: "management-card spaces-card",
                        div { class: "management-card-head", h2 { "Rooms & suites" }
                            div { class: "room-add-actions",
                            button { class: "outline-button", onclick: move |_| {
                                let next = draft().as_ref().map(|p| p.spaces.iter().map(|s| s.id).max().unwrap_or(0) + 1).unwrap_or(1);
                                edit_property(draft, |p| { let mut room = Space::new(next); room.name = format!("Room {}", p.spaces.iter().filter(|s| s.kind == "Bedroom").count() + 1); p.spaces.push(room); });
                                editing_space.set(Some(next));
                            }, "+ Room" }
                            button { class: "outline-button", onclick: move |_| {
                                let next = draft().as_ref().map(|p| p.spaces.iter().map(|s| s.id).max().unwrap_or(0) + 1).unwrap_or(1);
                                edit_property(draft, |p| { let mut suite = Space::new(next); suite.kind = "Suite".into(); suite.name = format!("Suite {}", p.spaces.iter().filter(|s| s.kind == "Suite").count() + 1); p.spaces.push(suite); });
                                editing_space.set(Some(next));
                            }, "+ Suite" }
                            }
                        }
                        div { class: "space-management-grid",
                            div { class: "space-management-list",
                                for space in item.spaces.iter().filter(|space| space.kind == "Bedroom" || space.kind == "Suite") {
                                    button { key: "{space.id}", class: if editing_space() == Some(space.id) { "space-management-row active" } else { "space-management-row" }, onclick: { let id = space.id; move |_| editing_space.set(Some(id)) },
                                        strong { "{space.name}" } small { if space.kind == "Bedroom" { "Room" } else { "Suite" } }
                                    }
                                }
                                if item.spaces.iter().all(|space| space.kind != "Bedroom" && space.kind != "Suite") { p { class: "empty", "No rooms yet." } }
                            }
                            if let Some(space) = selected_space {
                                div { class: "space-management-editor",
                                    div { class: "management-card-head", h3 { "{space.name}" } button { class: "text-button danger", onclick: { let id = space.id; move |_| { edit_property(draft, |p| { p.spaces.retain(|s| s.id != id); for child in &mut p.spaces { if child.parent_id == Some(id) { child.parent_id = None; } } }); editing_space.set(None); } }, "Remove" } }
                                    div { class: "management-fields",
                                        label { "Name", input { value: space.name.clone(), oninput: { let id = space.id; move |e| edit_space(draft, id, |s| s.name = e.value()) } } }
                                        label { "Type", select { value: space.kind.clone(), oninput: { let id = space.id; move |e| {
                                            let kind = e.value();
                                            edit_property(draft, |p| {
                                                if let Some(current) = p.spaces.iter_mut().find(|s| s.id == id) { current.kind = kind.clone(); current.parent_id = None; }
                                                if kind != "Suite" { for child in &mut p.spaces { if child.parent_id == Some(id) { child.parent_id = None; } } }
                                            });
                                        } },
                                            option { value: "Bedroom", "Room" } option { value: "Suite", "Suite" }
                                        } }
                                    }
                                    details { class: "room-extra",
                                        summary { "More room details" }
                                        div { class: "management-fields",
                                            label { "Level", input { value: space.level.clone(), oninput: { let id = space.id; move |e| edit_space(draft, id, |s| s.level = e.value()) } } }
                                            if space.kind == "Bedroom" { label { "Bathroom", select { value: space.bathroom_access.clone(), oninput: { let id = space.id; move |e| edit_space(draft, id, |s| s.bathroom_access = e.value()) }, option { value: "", "Not recorded" } option { value: "Ensuite", "Ensuite" } option { value: "Separate", "Separate" } } } }
                                            label { "Amenities", textarea { rows: "2", value: space.amenities.join(", "), oninput: { let id = space.id; move |e| edit_space(draft, id, |s| s.amenities = e.value().split(',').map(str::trim).filter(|x| !x.is_empty()).map(str::to_string).collect()) } } }
                                            label { "Notes", textarea { rows: "2", value: if space.notes.starts_with("Imported from the earlier catalog count") { String::new() } else { space.notes.clone() }, oninput: { let id = space.id; move |e| edit_space(draft, id, |s| s.notes = e.value()) } } }
                                        }
                                    }
                                    h3 { "Photos" }
                                    PhotoManager { draft, room_id: Some(space.id), uploading, message }
                                    if space.kind == "Bedroom" || space.kind == "Suite" {
                                        div { class: "room-pricing-head", h3 { "Pricing" } a { href: "/pricing", "All pricing →" } }
                                        div { class: "management-prices",
                                            label { "Base monthly rent", input { r#type: "number", min: "1", value: space.current_monthly_rent.or(space.rent_sep_dec).or(space.rent_jan_apr).or(space.rent_may_aug).map(|v| v.to_string()).unwrap_or_default(), oninput: { let id = space.id; move |e| edit_space(draft, id, |s| { s.current_monthly_rent = e.value().parse().ok(); }) } } }
                                        }
                                    }
                                }
                            } else { p { class: "empty", "Choose a room or suite." } }
                        }
                    }
                } else if !loading() { p { class: "availability-message", "{message}" } }
            }
        }
    }
}
