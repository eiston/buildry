use crate::{
    api, map,
    model::{Property, Space},
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

fn photo_lines(value: String) -> Vec<String> {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
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
    let mut uploading = use_signal(|| false);
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
            .find(|space| Some(space.id) == editing_space())
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
                            h2 { "Property details" }
                            div { class: "management-fields",
                                label { "Property name", input { value: item.name.clone(), oninput: move |e| edit_property(draft, |p| p.name = e.value()) } }
                                label { "Street address", input { value: item.address.clone(), oninput: move |e| edit_property(draft, |p| { p.address = e.value(); p.latitude = None; p.longitude = None; p.map_label = None; }) } }
                                label { "Area", input { value: item.area.clone(), oninput: move |e| edit_property(draft, |p| p.area = e.value()) } }
                                label { "Confirmed amenities (comma separated)", textarea { rows: "2", value: item.amenities.join(", "), oninput: move |e| edit_property(draft, |p| p.amenities = e.value().split(',').map(str::trim).filter(|x| !x.is_empty()).map(str::to_string).collect()) } }
                                label { "Notes", textarea { rows: "4", value: item.notes.clone(), oninput: move |e| edit_property(draft, |p| p.notes = e.value()) } }
                            }
                            div { class: "location-management",
                                h3 { "Map location" }
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
                            h2 { "House images" }
                            p { class: "management-help", "Upload photos or add image URLs below. The first image is the cover shown to guests. Save the property after uploading." }
                            label { "Upload house photos", input { r#type: "file", accept: "image/jpeg,image/png,image/webp", multiple: true, onchange: move |e| {
                                let files = e.files();
                                if files.is_empty() { return; }
                                uploading.set(true);
                                spawn(async move {
                                    for file in files {
                                        message.set(format!("Uploading {}…", file.name()));
                                        match api::upload_image(file).await {
                                            Ok(url) => { edit_property(draft, |p| p.photos.push(url)); message.set("Photo uploaded. Save the property to keep it in the gallery.".into()); }
                                            Err(error) => { message.set(error); break; }
                                        }
                                    }
                                    uploading.set(false);
                                });
                            } } }
                            if !item.photos.is_empty() { div { class: "photo-preview", for (index, photo) in item.photos.iter().enumerate() { img { key: "{index}", src: "{photo}", alt: "House photo {index + 1}" } } } }
                            label { "Image URLs, in display order", textarea { rows: "5", value: item.photos.join("\n"), oninput: move |e| edit_property(draft, |p| p.photos = photo_lines(e.value())) } }
                        }
                    }
                    section { class: "management-card spaces-card",
                        div { class: "management-card-head", div { h2 { "Rooms & areas" } p { class: "management-help", "Manage layout, room photos, and rent for this property." } }
                            button { class: "outline-button", onclick: move |_| {
                                let next = draft().as_ref().map(|p| p.spaces.iter().map(|s| s.id).max().unwrap_or(0) + 1).unwrap_or(1);
                                edit_property(draft, |p| p.spaces.push(Space::new(next)));
                                editing_space.set(Some(next));
                            }, "+ Add space" }
                        }
                        div { class: "space-management-grid",
                            div { class: "space-management-list",
                                for space in item.spaces.iter() {
                                    button { key: "{space.id}", class: if editing_space() == Some(space.id) { "space-management-row active" } else { "space-management-row" }, onclick: { let id = space.id; move |_| editing_space.set(Some(id)) },
                                        strong { "{space.name}" } small { "{space.kind}" }
                                    }
                                }
                                if item.spaces.is_empty() { p { class: "empty", "No spaces yet." } }
                            }
                            if let Some(space) = selected_space {
                                div { class: "space-management-editor",
                                    div { class: "management-card-head", h3 { "Edit space" } button { class: "text-button danger", onclick: { let id = space.id; move |_| { edit_property(draft, |p| { p.spaces.retain(|s| s.id != id); for child in &mut p.spaces { if child.parent_id == Some(id) { child.parent_id = None; } } }); editing_space.set(None); } }, "Remove space" } }
                                    div { class: "management-fields",
                                        label { "Space name", input { value: space.name.clone(), oninput: { let id = space.id; move |e| edit_space(draft, id, |s| s.name = e.value()) } } }
                                        label { "Type", select { value: space.kind.clone(), oninput: { let id = space.id; move |e| {
                                            let kind = e.value();
                                            edit_property(draft, |p| {
                                                if let Some(current) = p.spaces.iter_mut().find(|s| s.id == id) { current.kind = kind.clone(); if kind == "Area" || kind == "Suite" { current.parent_id = None; } }
                                                if kind != "Area" && kind != "Suite" { for child in &mut p.spaces { if child.parent_id == Some(id) { child.parent_id = None; } } }
                                            });
                                        } },
                                            option { value: "Bedroom", "Bedroom" } option { value: "Bathroom", "Bathroom" } option { value: "Area", "Area / group" } option { value: "Suite", "Suite" } option { value: "Kitchen", "Kitchen" } option { value: "Living space", "Living space" } option { value: "Other", "Other" }
                                        } }
                                        if space.kind != "Area" && space.kind != "Suite" { label { "Inside area", select { value: space.parent_id.map(|id| id.to_string()).unwrap_or_default(), oninput: { let id = space.id; move |e| edit_space(draft, id, |s| s.parent_id = e.value().parse().ok()) },
                                            option { value: "", "Whole property" }
                                            for area in item.spaces.iter().filter(|s| (s.kind == "Area" || s.kind == "Suite") && s.id != space.id) { option { key: "{area.id}", value: "{area.id}", "{area.name}" } }
                                        } } }
                                        label { "Level", input { value: space.level.clone(), oninput: { let id = space.id; move |e| edit_space(draft, id, |s| s.level = e.value()) } } }
                                        if space.kind == "Bedroom" { label { "Bathroom access", select { value: space.bathroom_access.clone(), oninput: { let id = space.id; move |e| edit_space(draft, id, |s| s.bathroom_access = e.value()) }, option { value: "", "Not recorded" } option { value: "Ensuite", "Ensuite" } option { value: "Separate", "Separate bathroom" } } } }
                                        label { "Room amenities (comma separated)", textarea { rows: "2", value: space.amenities.join(", "), oninput: { let id = space.id; move |e| edit_space(draft, id, |s| s.amenities = e.value().split(',').map(str::trim).filter(|x| !x.is_empty()).map(str::to_string).collect()) } } }
                                        label { "Notes", textarea { rows: "2", value: space.notes.clone(), oninput: { let id = space.id; move |e| edit_space(draft, id, |s| s.notes = e.value()) } } }
                                    }
                                    h3 { "Room images" }
                                    p { class: "management-help", "The first image is the room cover. Upload photos or add one URL per line, then save the property." }
                                    label { "Upload room photos", input { r#type: "file", accept: "image/jpeg,image/png,image/webp", multiple: true, onchange: { let id = space.id; move |e| {
                                        let files = e.files();
                                        if files.is_empty() { return; }
                                        uploading.set(true);
                                        spawn(async move {
                                            for file in files {
                                                message.set(format!("Uploading {}…", file.name()));
                                                match api::upload_image(file).await {
                                                    Ok(url) => { edit_space(draft, id, |s| s.photos.push(url)); message.set("Photo uploaded. Save the property to keep it in the room gallery.".into()); }
                                                    Err(error) => { message.set(error); break; }
                                                }
                                            }
                                            uploading.set(false);
                                        });
                                    } } } }
                                    if !space.photos.is_empty() { div { class: "photo-preview", for (index, photo) in space.photos.iter().enumerate() { img { key: "{index}", src: "{photo}", alt: "Room photo {index + 1}" } } } }
                                    label { "Image URLs", textarea { rows: "3", value: space.photos.join("\n"), oninput: { let id = space.id; move |e| edit_space(draft, id, |s| s.photos = photo_lines(e.value())) } } }
                                    if space.kind == "Bedroom" || space.kind == "Suite" {
                                        h3 { "Pricing" }
                                        p { class: "management-help", "Seasonal rent must decrease from September to January to May. All property rates can also be edited on the Pricing page." }
                                        div { class: "management-prices",
                                            label { "Current rent / month", input { r#type: "number", min: "0", value: space.current_monthly_rent.map(|v| v.to_string()).unwrap_or_default(), oninput: { let id = space.id; move |e| edit_space(draft, id, |s| { s.current_monthly_rent = e.value().parse().ok(); s.rent_is_estimate = false; }) } } }
                                            label { "Sep–Dec / month", input { r#type: "number", min: "1", value: space.rent_sep_dec.map(|v| v.to_string()).unwrap_or_default(), oninput: { let id = space.id; move |e| edit_space(draft, id, |s| { s.rent_sep_dec = e.value().parse().ok(); s.seasonal_prices_confirmed = false; }) } } }
                                            label { "Jan–Apr / month", input { r#type: "number", min: "1", value: space.rent_jan_apr.map(|v| v.to_string()).unwrap_or_default(), oninput: { let id = space.id; move |e| edit_space(draft, id, |s| { s.rent_jan_apr = e.value().parse().ok(); s.seasonal_prices_confirmed = false; }) } } }
                                            label { "May–Aug / month", input { r#type: "number", min: "1", value: space.rent_may_aug.map(|v| v.to_string()).unwrap_or_default(), oninput: { let id = space.id; move |e| edit_space(draft, id, |s| { s.rent_may_aug = e.value().parse().ok(); s.seasonal_prices_confirmed = false; }) } } }
                                        }
                                        label { class: "check-label", input { r#type: "checkbox", checked: space.seasonal_prices_confirmed, onchange: { let id = space.id; move |e| edit_space(draft, id, |s| s.seasonal_prices_confirmed = e.checked()) } } "These seasonal rates are confirmed" }
                                    }
                                }
                            } else { p { class: "empty", "Choose a space to edit its details." } }
                        }
                    }
                } else if !loading() { p { class: "availability-message", "{message}" } }
            }
        }
    }
}
