use crate::icon::Icon;
use crate::{
    address_picker::{AddressField, AddressSearchScreen},
    api,
    model::{Property, Space},
    photo_manager::PhotoManager,
    CSS,
};
use dioxus::prelude::*;
use gloo_net::http::Request;
use serde::Deserialize;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

fn add_room(draft: Signal<Option<Property>>, mut editing_space: Signal<Option<u32>>) {
    let next = draft()
        .as_ref()
        .map(|p| p.spaces.iter().map(|s| s.id).max().unwrap_or(0) + 1)
        .unwrap_or(1);
    edit_property(draft, |p| {
        let mut room = Space::new(next);
        room.name = format!(
            "Room {}",
            p.spaces.iter().filter(|s| s.kind == "Bedroom").count() + 1
        );
        p.spaces.push(room);
    });
    editing_space.set(Some(next));
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
}

fn deliverable_email(email: &str) -> bool {
    let email = email.trim();
    email.contains('@')
        && !email.chars().any(char::is_whitespace)
        && !email.to_ascii_lowercase().ends_with("@example.invalid")
}

#[component]
fn AmenitiesEditor(mut draft: Signal<Option<Property>>, room_id: Option<u32>) -> Element {
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
        crate::collapsible_panel::CollapsibleCard { class: "amenities-editor", summary_class: "amenities-heading", summary: rsx! {
                div { h3 { "What this place offers" } p { "Choose the features guests can expect." } }
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
                                button { r#type: "button", class: if amenities.iter().any(|item| item.eq_ignore_ascii_case(choice)) { "amenity-choice selected" } else { "amenity-choice" },
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
                            button { r#type: "button", aria_label: "Remove {item}", onclick: { let item = item.clone(); move |_| {
                                match room_id {
                                    Some(id) => edit_space(draft, id, |space| space.amenities.retain(|value| value != &item)),
                                    None => edit_property(draft, |property| property.amenities.retain(|value| value != &item)),
                                }
                            } }, Icon { name: "close" } }
                        }
                    }
                }
            }
            crate::app_form::AppForm { class: "amenity-add", onsubmit: move |event: FormEvent| {
                event.prevent_default();
                let value = new_amenity().trim().to_string();
                if value.is_empty() || value.len() > 80 { return; }
                let apply = |items: &mut Vec<String>| { if items.len() < 30 && !items.iter().any(|item| item.eq_ignore_ascii_case(&value)) { items.push(value.clone()); } };
                match room_id {
                    Some(id) => edit_space(draft, id, |space| apply(&mut space.amenities)),
                    None => edit_property(draft, |property| apply(&mut property.amenities)),
                }
                new_amenity.set(String::new());
            },
                crate::form_input::FormInput { aria_label: "Add an amenity", placeholder: "Add another amenity", value: "{new_amenity}", oninput: move |event: FormEvent| new_amenity.set(event.value()) }
                button { r#type: "submit", class: "amenity-add-button", aria_label: "Add amenity", title: "Add amenity", Icon { name: "return" } }
            }
            }
        }
    }
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
    let auto_pending = use_signal(|| false);
    let auto_running = use_signal(|| false);
    let uploading = use_signal(|| false);
    let mut message = use_signal(String::new);
    let mut address_mode = use_signal(|| false);
    let mut room_add_out_of_view = use_signal(|| false);
    let mut staff_managers = use_signal(Vec::<StaffManager>::new);
    let mut staff_error = use_signal(String::new);
    let mut can_manage = use_signal(|| is_new);
    let mut access_loaded = use_signal(|| is_new);

    use_effect(move || {
        if let Some(property_id) = requested_id {
            spawn(async move {
                if let Ok(response) = Request::get(&format!("/api/properties/{property_id}/access")).send().await {
                    if let Ok(access) = response.json::<PropertyAccess>().await { can_manage.set(access.can_manage); }
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
    let available_manager_count = property
        .as_ref()
        .map(|item| {
            usize::from(
                item.owner_manages_tours
                    && item.owner_id.is_some()
                    && deliverable_email(&item.owner_email),
            ) + staff_managers()
                .iter()
                .filter(|staff| {
                    staff.active
                        && item.manager_staff_ids.contains(&staff.id)
                        && deliverable_email(&staff.email)
                })
                .count()
        })
        .unwrap_or(0);
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
                onselect: move |selection: crate::address_picker::AddressSelection| {
                    edit_property(draft, |p| { p.address = selection.label.clone(); p.latitude = selection.latitude; p.longitude = selection.longitude; p.map_label = selection.latitude.map(|_| selection.label.clone()); });
                    address_mode.set(false);
                    if !is_new { queue_property_autosave(draft, auto_pending, auto_running, message); }
                }
            }
        };
    }
    rsx! {
        document::Title { "Manage property | Buildry" }
        document::Stylesheet { href: CSS }
        crate::admin_shell::AdminShell { active: "/", property_count: None, main_class: "main-content property-admin",
                crate::form_action_bar::FormActionBar { back_href: "/".to_string(), back_label: "Back to all properties".to_string(),title:property.as_ref().map(|item|item.name.clone()),
                    if property.is_some() {
                        if room_add_out_of_view() {
                            button { class: "form-action-icon form-action-add", r#type: "button", aria_label: "Add room", title: "Add room", onclick: move |_| add_room(draft, editing_space), Icon { name: "plus" } }
                        }
                        button { class: "property-save-button", aria_label: if saving() || auto_running() { "Saving property" } else if uploading() { "Uploading photos" } else { "Save property" }, title: if saving() || auto_running() { "Saving property" } else if uploading() { "Uploading photos" } else { "Save property" }, disabled: saving() || auto_running() || uploading(), onclick: move |_| {
                            if saving() || auto_running() || uploading() { return; }
                            let Some(mut current) = draft() else { return; };
                            current.name = current.name.trim().to_string();
                            current.external_name = current.external_name.trim().to_string();
                            current.address = current.address.trim().to_string();
                            for space in &mut current.spaces { space.external_name = space.external_name.trim().to_string(); }
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
                        }, Icon { name: "check" } }
                    }
                }
                if loading() { p { class: "stays-state", "Loading property…" } }
                if let Some(item) = property {
                    div { class: "page-head",
                        div { class: "property-heading", div { h1 { if is_new { "Add property" } else { "{item.name}" } } if is_new || item.name != item.address { p { class: "catalog-summary", if item.address.trim().is_empty() { "Address not set" } else { "{item.address}" } } } } }
                    }
                    if !message().is_empty() { p { class: "availability-message", role: "status", "{message}" } }
                    div { class: "management-grid",
                        crate::collapsible_panel::FormCard { title: "General".to_string(), class: Some("management-card property-edit-panel".to_string()),
                            crate::app_form::FormRows { class: Some("management-fields".to_string()),
                                div { class: "property-manager-field",
                                    div { class: "property-manager-field-heading", span { class: "property-manager-field-label", "Owner" } }
                                    strong { "{item.owner_name}" }
                                }
                                crate::inline_text_field::InlineTextField { label: "Property name", value: item.name.clone(), oncommit: move |text| { edit_property(draft, |p| p.name = text); if !is_new { queue_property_autosave(draft, auto_pending, auto_running, message); } } }
                                crate::inline_text_field::InlineTextField { label: "External name", value: item.external_name.clone(), placeholder: "Shown to guests; generated if blank", maxlength: 100, oncommit: move |text| { edit_property(draft, |p| p.external_name = text); if !is_new { queue_property_autosave(draft, auto_pending, auto_running, message); } } }
                                AddressField { label: "Street address", value: item.address.clone(), onopen: move |_| address_mode.set(true) }
                                label { "Notes", crate::notebook_textarea::NotebookTextarea { label: "Property notes", value: if item.notes.starts_with("Room breakdown entered from the supplied report.") || item.notes.starts_with("Address supplied.") { String::new() } else { item.notes.clone() }, onchange: move |text| { edit_property(draft, |p| p.notes = text); if !is_new { queue_property_autosave(draft, auto_pending, auto_running, message); } } } }
                                div { class: "property-manager-field",
                                    div { class: "property-manager-field-heading", span { class: "property-manager-field-label", "Property managers" } small { "Assigned managers can work on this property, host tours, and receive tasks." } }
                                    if can_manage() { crate::form_multi_select::FormMultiSelect {
                                        label: "Property managers", class: "property-manager-multiselect", search_placeholder: "Search people", show_done: true, show_selected_names: true,
                                        selected: std::iter::once((item.owner_manages_tours && item.owner_id.is_some()).then_some("owner".to_string())).flatten().chain(item.manager_staff_ids.iter().map(|id| format!("staff:{id}"))).collect(),
                                        options: std::iter::once(item.owner_id.map(|_| { let mut option = crate::form_multi_select::MultiSelectOption::new("owner".into(), item.owner_name.clone(), String::new()); option.description = "Property owner".into(); option })).flatten()
                                            .chain(staff_managers().into_iter().filter(|staff| staff.active || item.manager_staff_ids.contains(&staff.id)).map(|staff| { let mut option = crate::form_multi_select::MultiSelectOption::new(format!("staff:{}", staff.id), staff.name.clone(), staff.photo_url.clone()); option.description = if !staff.active {"Inactive staff"} else if !deliverable_email(&staff.email) {"Email required before assigning"} else {"Staff"}.into(); option.disabled = !deliverable_email(&staff.email) && !item.manager_staff_ids.contains(&staff.id); option })).collect(),
                                        onchange: move |values: Vec<String>| {
                                            edit_property(draft, |p| {
                                                p.owner_manages_tours = values.iter().any(|value| value == "owner");
                                                p.manager_staff_ids = values.iter().filter_map(|value| value.strip_prefix("staff:").and_then(|id| id.parse().ok())).collect();
                                            });
                                            if !is_new { queue_property_autosave(draft, auto_pending, auto_running, message); }
                                        },
                                    } } else { small { "Property manager assignments are managed by the owner or an administrator." } }
                                    if can_manage() && available_manager_count == 0 { small { class: "property-manager-warning", "Choose a manager with a real email address to offer tour times." } }
                                    if !staff_error().is_empty() { small { role: "alert", "{staff_error}" } }
                                }
                            }
                            AmenitiesEditor { draft, room_id: None }
                        }
                        crate::collapsible_panel::FormCard { title: "Photos".to_string(), class: Some("management-card property-edit-panel".to_string()),
                            PhotoManager { draft, room_id: None, uploading, message }
                        }
                    }
                    section { class: "rooms-workspace",
                        div { class: "rooms-heading-card",
                            h2 { "Rooms" }
                            button { class: "outline-button room-add-button", r#type: "button", onmounted: move |_: MountedEvent| {
                                let update = Closure::<dyn FnMut()>::wrap(Box::new(move || {
                                    let hidden = web_sys::window()
                                        .and_then(|window| window.document())
                                        .and_then(|document| document.query_selector(".room-add-button").ok().flatten())
                                        .map(|button| button.get_bounding_client_rect().bottom() <= 68.0)
                                        .unwrap_or(false);
                                    room_add_out_of_view.set(hidden);
                                }));
                                if let Some(window) = web_sys::window() {
                                    let _ = window.add_event_listener_with_callback("scroll", update.as_ref().unchecked_ref());
                                    update.forget();
                                }
                            }, onclick: move |_| add_room(draft, editing_space), Icon { name: "plus" } span { "Add a room" } }
                            }
                        div { class: "space-management-grid",
                            div { class: "space-management-list", role: "tablist", aria_label: "Rooms",
                                for space in item.spaces.iter().filter(|space| space.kind == "Bedroom" || space.kind == "Suite") {
                                    button { key: "{space.id}", role: "tab", aria_selected: editing_space() == Some(space.id), class: if editing_space() == Some(space.id) { "space-management-row active" } else { "space-management-row" }, onclick: { let id = space.id; move |_| editing_space.set(Some(id)) },
                                        strong { "{space.name}" } small { if space.kind == "Bedroom" { "Room" } else { "Suite" } }
                                    }
                                }
                                if item.spaces.iter().all(|space| space.kind != "Bedroom" && space.kind != "Suite") { p { class: "empty", "No rooms yet." } }
                            }
                            crate::collapsible_panel::FormCard { title: "Room details".to_string(), class: Some("management-card space-management-card property-edit-panel".to_string()),
                            if let Some(space) = selected_space {
                                div { key: "{space.id}", class: "space-management-editor",
                                    div { class: "management-card-head", button { class: "text-button danger", onclick: { let id = space.id; let name = space.name.clone(); move |_| {
                                        let confirmed = web_sys::window()
                                            .and_then(|window| window.confirm_with_message(&format!("Remove {name}? This change takes effect when you save the property.")).ok())
                                            .unwrap_or(false);
                                        if !confirmed { return; }
                                        edit_property(draft, |p| { p.spaces.retain(|s| s.id != id); for child in &mut p.spaces { if child.parent_id == Some(id) { child.parent_id = None; } } });
                                        editing_space.set(None);
                                        message.set("Room removed. Save to keep changes.".into());
                                    } }, "Remove" } }
                                    crate::app_form::FormRows { class: Some("management-fields".to_string()),
                                        { let id = space.id; rsx! { crate::inline_text_field::InlineTextField { label: "Name", value: space.name.clone(), oncommit: move |text| { edit_space(draft, id, |s| s.name = text); queue_property_autosave(draft, auto_pending, auto_running, message); } } } }
                                        { let id = space.id; rsx! { crate::inline_text_field::InlineTextField { label: "External name", value: space.external_name.clone(), placeholder: "Shown to guests; generated if blank", maxlength: 100, oncommit: move |text| { edit_space(draft, id, |s| s.external_name = text); queue_property_autosave(draft, auto_pending, auto_running, message); } } } }
                                    }
                                    crate::collapsible_panel::CollapsibleCard { class: "room-extra", summary: rsx! { "More room details" },
                                        crate::app_form::FormRows { class: Some("management-fields".to_string()),
                                            if space.kind == "Bedroom" { { let id = space.id; rsx! { crate::form_select::FormSelect { label: "Bathroom", required: false, value: space.bathroom_access.clone(), placeholder: "None", options: vec![("".into(),"None".into()),("Ensuite".into(),"Ensuite".into()),("Separate".into(),"Separate".into())], onchange: move |value:String| { edit_space(draft, id, |s| { s.bathroom_access = value.clone(); if !value.is_empty() { s.amenities.retain(|amenity| !["Ensuite bathroom", "Separate bathroom", "Private bathroom", "Shared bathroom"].iter().any(|access| amenity.eq_ignore_ascii_case(access))); } }); queue_property_autosave(draft, auto_pending, auto_running, message); } } } } }
                                            label { "Notes", crate::notebook_textarea::NotebookTextarea { label: "Room notes", value: if space.notes.starts_with("Imported from the earlier catalog count") { String::new() } else { space.notes.clone() }, onchange: { let id = space.id; move |text| { edit_space(draft, id, |s| s.notes = text); queue_property_autosave(draft, auto_pending, auto_running, message); } } } }
                                        }
                                    }
                                    AmenitiesEditor { draft, room_id: Some(space.id) }
                                    PhotoManager { draft, room_id: Some(space.id), uploading, message }
                                    if space.kind == "Bedroom" || space.kind == "Suite" {
                                        div { class: "room-pricing-head", a { href: "/pricing", span { "All pricing" } Icon { name: "arrow-right" } } }
                                        crate::app_form::FormRows { class: Some("management-prices".to_string()),
                                            { let id = space.id; rsx! { crate::inline_text_field::InlineTextField { label: "Base monthly rent", input_type: "number", min: "1", value: space.current_monthly_rent.or(space.rent_sep_dec).or(space.rent_jan_apr).or(space.rent_may_aug).map(|v| v.to_string()).unwrap_or_default(), oncommit: move |text: String| { edit_space(draft, id, |s| s.current_monthly_rent = text.parse().ok()); queue_property_autosave(draft, auto_pending, auto_running, message); } } } }
                                        }
                                    }
                                }
                            } else { p { class: "empty", "Choose a room or suite." } }
                            }
                        }
                    }
                } else if !loading() { p { class: "availability-message", "{message}" } }

        }
    }
}
