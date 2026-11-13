use crate::components::icon::Icon;
use crate::{
    components::forms::address_picker::{AddressField, AddressSearchScreen},
    services::api,
    model::{Property, Space},
    components::photo_manager::PhotoManager,
    CSS,
};
use dioxus::prelude::*;
use gloo_net::http::Request;
use serde::Deserialize;

// Persist property edits before opening the separate room creation page.
fn open_new_room(
    draft: Signal<Option<Property>>,
    is_new: bool,
    mut saving: Signal<bool>,
    mut message: Signal<String>,
) {
    let Some(mut current) = draft() else { return; };
    if current.name.trim().is_empty() || current.address.trim().is_empty() {
        message.set("Enter a property name and street address.".into());
        return;
    }
    current.name = current.name.trim().to_string();
    current.address = current.address.trim().to_string();
    message.set(String::new());
    saving.set(true);
    spawn(async move {
        match if is_new { api::create(&current).await } else { api::save(&current).await } {
            Ok(saved) => {
                if let Some(window) = web_sys::window() {
                    let _ = window.location().set_href(&format!("/properties/{}/rooms/new", saved.id));
                }
            }
            Err(error) => message.set(error),
        }
        saving.set(false);
    });
}

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

// Keep writes in order so a second blur cannot be overwritten by an older request.
fn queue_property_autosave(
    draft: Signal<Option<Property>>,
    mut pending: Signal<bool>,
    mut running: Signal<bool>,
    mut message: Signal<String>,
) {
    pending.set(true);
    if running() {
        return;
    }
    running.set(true);
    message.set("Saving…".into());
    spawn(async move {
        while pending() {
            pending.set(false);
            let Some(current) = draft() else {
                break;
            };
            if current.name.trim().is_empty()
                || current.address.trim().is_empty()
                || current
                    .spaces
                    .iter()
                    .any(|space| space.name.trim().is_empty())
            {
                message.set("Add a property name, address, and name for every room before changes can be saved.".into());
                break;
            }
            if let Err(error) = api::save(&current).await {
                message.set(format!("Could not save changes: {error}"));
                break;
            }
            if !pending() {
                message.set("Saved".into());
            }
        }
        running.set(false);
    });
}

const HOUSE_AMENITY_GROUPS: &[(&str, &[&str])] = &[
    (
        "Essentials",
        &["Wi-Fi", "Heating", "Air conditioning", "Laundry"],
    ),
    (
        "Kitchen & dining",
        &[
            "Kitchen",
            "Refrigerator",
            "Microwave",
            "Dishwasher",
            "Dining area",
        ],
    ),
    (
        "Living & outdoor",
        &["Living area", "TV", "Backyard", "Patio or balcony"],
    ),
    (
        "Parking & access",
        &["Parking", "Free street parking", "Private entrance"],
    ),
];

const ROOM_AMENITY_GROUPS: &[(&str, &[&str])] = &[
    (
        "Essentials",
        &["Wi-Fi", "Heating", "Air conditioning", "Laundry"],
    ),
    (
        "Sleeping & storage",
        &["Bed linens", "Closet", "Dresser", "Blackout curtains"],
    ),
    ("Bathroom fixtures", &["Shower", "Bathtub"]),
    (
        "Work & entertainment",
        &["Desk", "Dedicated workspace", "TV"],
    ),
];

#[derive(Clone, PartialEq, Deserialize)]
struct StaffManager {
    id: i64,
    name: String,
    email: String,
    active: bool,
    #[serde(default)]
    photo_url: String,
}

#[derive(Clone, PartialEq, Deserialize)]
struct PropertyAccess {
    can_manage: bool,
    #[serde(default)] capabilities:Vec<String>,
}



#[component]
fn AmenitiesEditor(mut draft: Signal<Option<Property>>, room_id: Option<u32>, on_change: EventHandler<()>) -> Element {
    let mut new_amenity = use_signal(String::new);
    let amenities = draft()
        .as_ref()
        .map(|property| match room_id {
            Some(id) => property
                .spaces
                .iter()
                .find(|space| space.id == id)
                .map(|space| space.amenities.clone())
                .unwrap_or_default(),
            None => property.amenities.clone(),
        })
        .unwrap_or_default();
    let groups = if room_id.is_some() {
        ROOM_AMENITY_GROUPS
    } else {
        HOUSE_AMENITY_GROUPS
    };
    let common_choices: Vec<&str> = groups
        .iter()
        .flat_map(|(_, choices)| choices.iter().copied())
        .collect();
    rsx! {
        crate::components::collapsible_panel::CollapsibleCard { class: "amenities-editor", summary_class: "amenities-heading", summary: rsx! {
                div { h3 { "What this place offers" } }
                span { class: "amenities-count", "{amenities.len()} selected" }
                span { class: "amenities-chevron", aria_hidden: "true", Icon { name: "chevron-down" } }
            },
            div { class: "amenities-body",
            div { class: "amenities-groups",
                for (category, options) in groups.iter() {
                    section { class: "amenity-group",
                        h4 { "{category}" }
                        div { class: "amenity-group-options",
                            for &choice in options.iter() {
                                crate::components::forms::form_button::FormButton { r#type: "button", class: if amenities.iter().any(|item| item.eq_ignore_ascii_case(choice)) { "amenity-choice selected" } else { "amenity-choice" },
                                    aria_pressed: amenities.iter().any(|item| item.eq_ignore_ascii_case(choice)),
                                    onclick: move |_| {
                                        let apply = |items: &mut Vec<String>| {
                                            if let Some(index) = items.iter().position(|item| item.eq_ignore_ascii_case(choice)) { items.remove(index); }
                                            else if items.len() < 30 { items.push(choice.to_string()); }
                                        };
                                        match room_id {
                                            Some(id) => edit_space(draft, id, |space| apply(&mut space.amenities)),
                                            None => edit_property(draft, |property| apply(&mut property.amenities)),
                                        }
                                        on_change.call(());
                                    },
                                    span { class: "amenity-choice-mark", if amenities.iter().any(|item| item.eq_ignore_ascii_case(choice)) { Icon { name: "check" } } }
                                    span { "{choice}" }
                                }
                            }
                        }
                    }
                }
            }
            if amenities.iter().any(|item| !common_choices.iter().any(|choice| item.eq_ignore_ascii_case(choice))) {
                div { class: "amenities-custom",
                    for item in amenities.iter().filter(|item| !common_choices.iter().any(|choice| item.eq_ignore_ascii_case(choice))) {
                        span { class: "amenity-custom-chip", "{item}"
                            crate::components::forms::form_button::FormButton { class: "", r#type: "button", aria_label: "Remove {item}", onclick: { let item = item.clone(); move |_| {
                                match room_id {
                                    Some(id) => edit_space(draft, id, |space| space.amenities.retain(|value| value != &item)),
                                    None => edit_property(draft, |property| property.amenities.retain(|value| value != &item)),
                                }
                                on_change.call(());
                            } }, Icon { name: "close" } }
                        }
                    }
                }
            }
            crate::components::forms::app_form::AppForm { class: "amenity-add", onsubmit: move |event: FormEvent| {
                event.prevent_default();
                let value = new_amenity().trim().to_string();
                if value.is_empty() || value.len() > 80 { return; }
                let apply = |items: &mut Vec<String>| { if items.len() < 30 && !items.iter().any(|item| item.eq_ignore_ascii_case(&value)) { items.push(value.clone()); } };
                match room_id {
                    Some(id) => edit_space(draft, id, |space| apply(&mut space.amenities)),
                    None => edit_property(draft, |property| apply(&mut property.amenities)),
                }
                new_amenity.set(String::new());
                on_change.call(());
            },
                crate::components::forms::form_input::FormInput { aria_label: "Add an amenity", placeholder: "Add another amenity", value: "{new_amenity}", oninput: move |event: FormEvent| new_amenity.set(event.value()) }
                crate::components::forms::form_button::FormButton { r#type: "submit", class: "amenity-add-button", aria_label: "Add amenity", title: "Add amenity", Icon { name: "return" } }
            }
            }
        }
    }
}

pub fn property_page() -> Element {
    let path = web_sys::window()
        .and_then(|window| window.location().pathname().ok())
        .unwrap_or_default();
    let segments: Vec<_> = path.trim_matches('/').split('/').collect();
    let requested_id = segments.get(1).and_then(|id| id.parse::<u32>().ok());
    let is_room_page = segments.get(2) == Some(&"rooms");
    let new_room = is_room_page && segments.get(3) == Some(&"new");
    let requested_room = if is_room_page { segments.get(3).and_then(|id| id.parse::<u32>().ok()) } else { None };
    let is_new = !is_room_page && segments.get(1) == Some(&"new");
    let mut draft = use_signal(|| None::<Property>);
    let mut editing_space = use_signal(|| None::<u32>);
    let mut loading = use_signal(|| true);
    let mut saving = use_signal(|| false);
    let mut remove_confirm = use_signal(|| false);
    let auto_pending = use_signal(|| false);
    let auto_running = use_signal(|| false);
    let uploading = use_signal(|| false);
    let mut message = use_signal(String::new);
    let mut address_mode = use_signal(|| false);
    let mut staff_managers = use_signal(Vec::<StaffManager>::new);
    let mut staff_error = use_signal(String::new);
    let mut can_price=use_signal(||is_new);
    let mut can_write=use_signal(||is_new);
    let mut can_manage = use_signal(|| is_new);
    let mut access_loaded = use_signal(|| is_new);

    use_effect(move || {
        if let Some(property_id) = requested_id {
            spawn(async move {
                if let Ok(response) = Request::get(&format!("/api/properties/{property_id}/access")).send().await {
                    if let Ok(access) = response.json::<PropertyAccess>().await { can_manage.set(access.can_manage);can_write.set(access.capabilities.iter().any(|v|v=="details.write"));can_price.set(access.capabilities.iter().any(|v|v=="pricing.write")); }
                }
                access_loaded.set(true);
            });
        }
    });

    use_effect(move || {
        spawn(async move {
            match Request::get("/api/property-managers/staff").send().await {
                Ok(response) if response.ok() => match response.json::<Vec<StaffManager>>().await {
                    Ok(staff) => staff_managers.set(staff),
                    Err(_) => staff_error.set("Could not read the staff list.".into()),
                },
                _ => staff_error.set("Could not load staff managers.".into()),
            }
        });
    });

    use_effect(move || {
        spawn(async move {
            match api::list().await {
                Ok(properties) => {
                    let mut selected = if is_new {
                        Some(Property::new(
                            properties.iter().map(|item| item.id).max().unwrap_or(0) + 1,
                        ))
                    } else {
                        properties
                            .into_iter()
                            .find(|item| Some(item.id) == requested_id)
                    };
                    if let Some(property) = selected.as_mut() {
                        for space in property
                            .spaces
                            .iter_mut()
                            .filter(|space| space.kind == "Bedroom")
                        {
                            if !space.bathroom_access.is_empty() {
                                space.amenities.retain(|amenity| {
                                    ![
                                        "Ensuite bathroom",
                                        "Separate bathroom",
                                        "Private bathroom",
                                        "Shared bathroom",
                                    ]
                                    .iter()
                                    .any(|access| amenity.eq_ignore_ascii_case(access))
                                });
                            }
                        }
                    }
                    if selected.is_none() {
                        message.set("Property not found.".into());
                    }
                    if new_room {
                        if let Some(property) = selected.as_mut() {
                            let id = property.spaces.iter().map(|space| space.id).max().unwrap_or(0) + 1;
                            let mut room = Space::new(id);
                            room.name = format!("Room {}", property.spaces.iter().filter(|space| space.kind == "Bedroom").count() + 1);
                            property.spaces.push(room);
                            editing_space.set(Some(id));
                        }
                    } else {
                        editing_space.set(requested_room);
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
            .find(|space| {
                Some(space.id) == editing_space()
                    && (space.kind == "Bedroom" || space.kind == "Suite")
            })
            .cloned()
    });
    if !access_loaded() {
        return rsx! { main { class: "main-content property-admin", "Loading property access…" } };
    }
    if address_mode() {
        return rsx! {
            AddressSearchScreen { initial_value: property.as_ref().map(|item| item.address.clone()).unwrap_or_default(),
                onback: move |_| address_mode.set(false),
                onselect: move |selection: crate::components::forms::address_picker::AddressSelection| {
                    edit_property(draft, |p| { p.address = selection.label.clone(); p.latitude = selection.latitude; p.longitude = selection.longitude; p.map_label = selection.latitude.map(|_| selection.label.clone()); });
                    address_mode.set(false);
                    if !is_new { queue_property_autosave(draft, auto_pending, auto_running, message); }
                }
            }
        };
    }
    rsx! {
        document::Title { if is_room_page { "Manage room | Buildry" } else { "Manage property | Buildry" } }
        document::Stylesheet { href: CSS }
        crate::layout::admin_shell::AdminShell { active: "/", property_count: None, main_class: "main-content property-admin",
            fieldset{class:"resource-fields",disabled:!can_write() || saving(),
                crate::components::forms::form_action_bar::FormActionBar { back_href: if is_room_page { format!("/properties/{}", requested_id.unwrap_or_default()) } else { "/properties".to_string() }, back_label: if is_room_page { "Back to property".to_string() } else { "Back to all properties".to_string() }, title: if is_room_page { selected_space.as_ref().map(|room| room.name.clone()) } else if is_new { Some("Add property".to_string()) } else { property.as_ref().map(|item|item.name.clone()) },
                    if can_write() && property.is_some() && (!is_room_page || selected_space.is_some()) {
                        if is_room_page && !new_room {
                            crate::components::forms::form_button::FormButton { class: "form-action-icon glass-icon-button danger", r#type: "button", aria_label: "Remove room", title: "Remove room", disabled: saving() || auto_running() || uploading(), onclick: move |_| remove_confirm.set(true), Icon { name: "trash" } }
                        }
                        crate::components::forms::form_button::FormButton { class: "property-save-button", aria_label: if saving() || auto_running() { if is_room_page { "Saving room" } else { "Saving property" } } else if uploading() { "Uploading photos" } else { if is_room_page { "Save room" } else { "Save property" } }, title: if saving() || auto_running() { if is_room_page { "Saving room" } else { "Saving property" } } else if uploading() { "Uploading photos" } else { if is_room_page { "Save room" } else { "Save property" } }, disabled: saving() || auto_running() || uploading(), onclick: move |_| {
                            if saving() || auto_running() || uploading() { return; }
                            let Some(mut current) = draft() else { return; };
                            current.name = current.name.trim().to_string();
                            current.external_name = current.external_name.trim().to_string();
                            current.address = current.address.trim().to_string();
                            for space in &mut current.spaces { space.external_name = space.external_name.trim().to_string(); }
                            if current.name.is_empty() || current.address.is_empty() { message.set("Enter a property name and street address.".into()); return; }
                            if let Some(space) = current.spaces.iter().find(|space| space.name.trim().is_empty()) { message.set(format!("Name every space before saving (space {}).", space.id)); return; }
                            if !is_new && !new_room {
                                draft.set(Some(current));
                                queue_property_autosave(draft, auto_pending, auto_running, message);
                                return;
                            }
                            saving.set(true);
                            message.set(if is_room_page { "Saving room…" } else { "Saving property…" }.into());
                            spawn(async move {
                                match if is_new {api::create(&current).await}else{api::save(&current).await} {
                                    Ok(saved) => {
                                        let id = saved.id;
                                        draft.set(Some(saved));
                                        message.set("Saved".into());
                                        if new_room { if let Some(window) = web_sys::window() { let _ = window.location().set_href(&format!("/properties/{id}/rooms/{}", editing_space().unwrap_or_default())); } }
                                        else if is_new { if let Some(window) = web_sys::window() { let _ = window.location().set_href(&format!("/properties/{id}")); } }
                                    }
                                    Err(error) => message.set(error),
                                }
                                saving.set(false);
                            });
                        }, Icon { name: "check" } }
                    }
                }
                if loading() { p { class: "stays-state", "Loading property…" } }
                if let Some(item) = property {
                    div { class: "page-head",
                        div { class: "property-heading", div { h1 { if is_room_page { if let Some(room) = selected_space.as_ref() { "{room.name}" } else { "Room not found" } } else if is_new { "Add property" } else { "{item.name}" } } if !is_room_page && (is_new || item.name != item.address) { p { class: "catalog-summary", if item.address.trim().is_empty() { "Address not set" } else { "{item.address}" } } } } }
                    }
                    if !message().is_empty() { p { class: "availability-message", role: "status", "{message}" } }
                    if !is_room_page {
                    div { class: "management-grid",
                        crate::components::collapsible_panel::FormCard { title: "General".to_string(), class: Some("management-card property-edit-panel".to_string()),
                            crate::components::forms::app_form::FormRows { class: Some("management-fields".to_string()),
                                crate::components::forms::form_select::FormValue { label: "Owner", value: item.owner_name.clone() }
                                crate::components::forms::inline_text_field::InlineTextField { disabled:!can_write(), label: "Property name", value: item.name.clone(), oncommit: move |text| { edit_property(draft, |p| p.name = text); if !is_new { queue_property_autosave(draft, auto_pending, auto_running, message); } } }
                                crate::components::forms::inline_text_field::InlineTextField { disabled:!can_write(), label: "External name", value: item.external_name.clone(), placeholder: "Shown to guests; generated if blank", maxlength: 100, oncommit: move |text| { edit_property(draft, |p| p.external_name = text); if !is_new { queue_property_autosave(draft, auto_pending, auto_running, message); } } }
                                AddressField { label: "Street address", value: item.address.clone(), onopen: move |_| address_mode.set(true) }
                                label { "Notes", crate::components::forms::notebook_textarea::NotebookTextarea { label: "Property notes", value: if item.notes.starts_with("Room breakdown entered from the supplied report.") || item.notes.starts_with("Address supplied.") { String::new() } else { item.notes.clone() }, onchange: move |text| { edit_property(draft, |p| p.notes = text); if !is_new { queue_property_autosave(draft, auto_pending, auto_running, message); } } } }
                                if !is_new { div { class: "property-manager-field",
                                    div { class: "property-manager-field-heading", span { class: "property-manager-field-label", "Property managers" } }
                                    if can_manage() { a{class:"outline-button",href:format!("/sharing/property/{}",item.id),"Share property"} if let Some(owner)=item.owner_id{a{class:"outline-button",href:format!("/landlords/view?id={owner}"),"Landlord profile"}} }
                                    if !staff_error().is_empty() { small { role: "alert", "{staff_error}" } }
                                } }
                            }
                            AmenitiesEditor { draft, room_id: None, on_change: move |_| { if !is_new { queue_property_autosave(draft, auto_pending, auto_running, message); } else { message.set("Unsaved changes".into()); } } }
                        }
                        crate::components::collapsible_panel::FormCard { title: "Photos".to_string(), class: Some("management-card property-edit-panel".to_string()),
                            PhotoManager { draft, room_id: None, uploading, message, on_change: move |_| { if !is_new { queue_property_autosave(draft, auto_pending, auto_running, message); } else { message.set("Unsaved changes".into()); } } }
                        }
                    }
                    section { class: "rooms-workspace",
                        div { class: "rooms-heading-card",
                            h2 { "Rooms" }
                            crate::components::forms::form_button::FormButton { class: "outline-button room-add-button", r#type: "button", disabled: saving() || auto_running() || uploading(), onclick: move |_| open_new_room(draft, is_new, saving, message), Icon { name: "plus" } span { "Add a room" } }
                        }
                        crate::components::searchable_list::JointList { class: "room-list",
                            for space in item.spaces.iter().filter(|space| space.kind == "Bedroom" || space.kind == "Suite") {
                                crate::components::searchable_list::ThumbnailListRow {
                                    key: "{space.id}", href: format!("/properties/{}/rooms/{}", item.id, space.id),
                                    name: space.name.clone(),
                                    subtitle: if space.kind == "Suite" { "Suite".into() } else if space.bathroom_access.is_empty() { "Room".into() } else { format!("{} bathroom", space.bathroom_access) },
                                    photo: space.photos.first().cloned(),
                                }
                            }
                            if item.spaces.iter().all(|space| space.kind != "Bedroom" && space.kind != "Suite") { p { class: "empty", "No rooms yet." } }
                        }
                    }
                    } else {
                            crate::components::collapsible_panel::FormCard { title: "Room details".to_string(), class: Some("management-card space-management-card property-edit-panel".to_string()),
                            if let Some(space) = selected_space.clone() {
                                div { key: "{space.id}", class: "space-management-editor",
                                    crate::components::forms::app_form::FormRows { class: Some("management-fields".to_string()),
                                        { let id = space.id; rsx! { crate::components::forms::inline_text_field::InlineTextField { disabled:!can_write(), label: "Name", value: space.name.clone(), oncommit: move |text| { edit_space(draft, id, |s| s.name = text); if !new_room { queue_property_autosave(draft, auto_pending, auto_running, message); } } } } }
                                        { let id = space.id; rsx! { crate::components::forms::inline_text_field::InlineTextField { disabled:!can_write(), label: "External name", value: space.external_name.clone(), placeholder: "Shown to guests; generated if blank", maxlength: 100, oncommit: move |text| { edit_space(draft, id, |s| s.external_name = text); if !new_room { queue_property_autosave(draft, auto_pending, auto_running, message); } } } } }
                                    }
                                    crate::components::collapsible_panel::CollapsibleCard { class: "room-extra", summary: rsx! { "More room details" },
                                        crate::components::forms::app_form::FormRows { class: Some("management-fields".to_string()),
                                            if space.kind == "Bedroom" { { let id = space.id; rsx! { crate::components::forms::form_select::FormSelect { label: "Bathroom", required: false, value: space.bathroom_access.clone(), placeholder: "None", options: vec![("".into(),"None".into()),("Ensuite".into(),"Ensuite".into()),("Separate".into(),"Separate".into())], onchange: move |value:String| { edit_space(draft, id, |s| { s.bathroom_access = value.clone(); if !value.is_empty() { s.amenities.retain(|amenity| !["Ensuite bathroom", "Separate bathroom", "Private bathroom", "Shared bathroom"].iter().any(|access| amenity.eq_ignore_ascii_case(access))); } }); if !new_room { queue_property_autosave(draft, auto_pending, auto_running, message); } } } } } }
                                            label { "Notes", crate::components::forms::notebook_textarea::NotebookTextarea { label: "Room notes", value: if space.notes.starts_with("Imported from the earlier catalog count") { String::new() } else { space.notes.clone() }, onchange: { let id = space.id; move |text| { edit_space(draft, id, |s| s.notes = text); if !new_room { queue_property_autosave(draft, auto_pending, auto_running, message); } } } } }
                                        }
                                    }
                                    AmenitiesEditor { draft, room_id: Some(space.id), on_change: move |_| { if !new_room { queue_property_autosave(draft, auto_pending, auto_running, message); } else { message.set("Unsaved changes".into()); } } }
                                    PhotoManager { draft, room_id: Some(space.id), uploading, message, on_change: move |_| { if !new_room { queue_property_autosave(draft, auto_pending, auto_running, message); } else { message.set("Unsaved changes".into()); } } }
                                    if space.kind == "Bedroom" || space.kind == "Suite" {
                                        div { class: "room-pricing-head", a { href: "/pricing", span { "All pricing" } Icon { name: "arrow-right" } } }
                                        crate::components::forms::app_form::FormRows { class: Some("management-prices".to_string()),
                                            { let id = space.id; rsx! { crate::components::forms::inline_text_field::InlineTextField { disabled:!can_price(), label: "Base monthly rent", input_type: "number", min: "1", value: space.current_monthly_rent.or(space.rent_sep_dec).or(space.rent_jan_apr).or(space.rent_may_aug).map(|v| v.to_string()).unwrap_or_default(), oncommit: move |text: String| { edit_space(draft, id, |s| s.current_monthly_rent = text.parse().ok()); if !new_room { queue_property_autosave(draft, auto_pending, auto_running, message); } } } } }
                                        }
                                    }
                                }
                            } else { p { class: "empty", "Room not found." } }
                            }
                    }
                } else if !loading() { p { class: "availability-message", "{message}" } }

        }
        if remove_confirm() {
            crate::components::shared_ui::ConfirmDialog {
                title: format!("Remove {}?", selected_space.as_ref().map(|space| space.name.as_str()).unwrap_or("room")),
                confirm_label: if saving() { "Removing…" } else { "Remove room" }, busy: saving(),
                on_close: move |_| { if !saving() { remove_confirm.set(false); } },
                on_confirm: move |_| {
                    if saving() || auto_running() || uploading() { return; }
                    let Some(id) = editing_space() else { return; };
                    let Some(mut current) = draft() else { return; };
                    current.spaces.retain(|space| space.id != id);
                    for child in &mut current.spaces { if child.parent_id == Some(id) { child.parent_id = None; } }
                    saving.set(true);
                    spawn(async move {
                        match if is_new {api::create(&current).await}else{api::save(&current).await} {
                            Ok(saved) => { if let Some(window) = web_sys::window() { let _ = window.location().set_href(&format!("/properties/{}", saved.id)); } }
                            Err(error) => { message.set(error); remove_confirm.set(false); }
                        }
                        saving.set(false);
                    });
                },
                children: rsx! {},
            }
        }

            }
    }
}
