use crate::{api, model::Property};
use dioxus::prelude::*;

fn photos_mut(property: &mut Property, room_id: Option<u32>) -> Option<&mut Vec<String>> {
    match room_id {
        Some(id) => property
            .spaces
            .iter_mut()
            .find(|room| room.id == id)
            .map(|room| &mut room.photos),
        None => Some(&mut property.photos),
    }
}

#[component]
pub fn PhotoManager(
    mut draft: Signal<Option<Property>>,
    room_id: Option<u32>,
    mut uploading: Signal<bool>,
    mut message: Signal<String>,
) -> Element {
    let mut url = use_signal(String::new);
    let photos = draft()
        .as_mut()
        .and_then(|property| photos_mut(property, room_id).cloned())
        .unwrap_or_default();
    rsx! {
        div { class: "photo-manager",
            div { class: "photo-manager-toolbar",
                label { class: "photo-upload", tabindex: "0",
                    span { class: "photo-upload-icon", "+" }
                    span { strong { "Upload photos" } small { "JPG, PNG or WebP · up to 5 MB each" } }
                    input { r#type: "file", accept: "image/jpeg,image/png,image/webp", multiple: true, disabled: uploading(), onchange: move |event| {
                        let files = event.files();
                        if files.is_empty() { return; }
                        uploading.set(true);
                        spawn(async move {
                            for file in files {
                                message.set(format!("Uploading {}…", file.name()));
                                match api::upload_image(file).await {
                                    Ok(path) => {
                                        if let Some(property) = draft.write().as_mut() {
                                            if let Some(list) = photos_mut(property, room_id) { list.push(path); }
                                        }
                                        message.set("Photo added. Save to keep changes.".into());
                                    }
                                    Err(error) => { message.set(error); break; }
                                }
                            }
                            uploading.set(false);
                        });
                    } }
                }
                details { class: "photo-url-panel",
                    summary { "Add image URL" }
                    div { class: "photo-url-form",
                        input { aria_label: "Image URL", placeholder: "https://… or /assets/…", value: "{url}", oninput: move |event| url.set(event.value()) }
                        button { r#type: "button", class: "outline-button", onclick: move |_| {
                            let value = url().trim().to_string();
                            if !(value.starts_with("https://") || value.starts_with("/assets/") || value.starts_with("/images/")) {
                                message.set("Use an HTTPS or site image URL.".into());
                                return;
                            }
                            if let Some(property) = draft.write().as_mut() {
                                if let Some(list) = photos_mut(property, room_id) { list.push(value); }
                            }
                            url.set(String::new());
                            message.set("Photo added. Save to keep changes.".into());
                        }, "Add" }
                    }
                }
            }
            if photos.is_empty() { p { class: "photo-empty", "No custom photos yet." } }
            else {
                div { class: "photo-tiles",
                    for (index, photo) in photos.iter().enumerate() {
                        div { key: "{index}", class: "photo-tile",
                            img { src: "{photo}", alt: "Photo {index + 1}" }
                            if index == 0 { span { class: "photo-cover", "Cover" } }
                            div { class: "photo-tile-actions",
                                button { r#type: "button", aria_label: "Move photo left", title: "Move left", disabled: index == 0, onclick: move |_| {
                                    if let Some(property) = draft.write().as_mut() { if let Some(list) = photos_mut(property, room_id) { list.swap(index, index - 1); } }
                                }, "←" }
                                button { r#type: "button", aria_label: "Move photo right", title: "Move right", disabled: index + 1 >= photos.len(), onclick: move |_| {
                                    if let Some(property) = draft.write().as_mut() { if let Some(list) = photos_mut(property, room_id) { list.swap(index, index + 1); } }
                                }, "→" }
                                button { r#type: "button", aria_label: "Remove photo", title: "Remove", onclick: move |_| {
                                    if let Some(property) = draft.write().as_mut() { if let Some(list) = photos_mut(property, room_id) { list.remove(index); } }
                                }, "×" }
                            }
                        }
                    }
                }
            }
        }
    }
}
