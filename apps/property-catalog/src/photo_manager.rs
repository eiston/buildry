use crate::icon::Icon;
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
    let mut viewing = use_signal(|| None::<usize>);
    let photos = draft()
        .as_mut()
        .and_then(|property| photos_mut(property, room_id).cloned())
        .unwrap_or_default();
    let title = if room_id.is_some() {
        "Room photos"
    } else {
        "House photos"
    };

    use_effect(move || {
        if let Some(index) = viewing() {
            if let Some(element) = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| {
                    document.get_element_by_id(&format!("gallery-viewer-photo-{room_id:?}-{index}"))
                })
            {
                element.scroll_into_view();
            }
        }
    });

    rsx! {
        div { class: "photo-manager",
            div { class: "gallery-heading",
                div { p { class: "gallery-eyebrow", "PHOTO GALLERY" } p { class: "gallery-hint", "Hold to reorder · First photo is the cover" } }
                label { class: "gallery-add", title: "Add photos", aria_label: "Add photos",
                    Icon { name: "plus" }
                    crate::form_input::FormFileInput { accept: "image/jpeg,image/png,image/webp", multiple: true, disabled: uploading(), onchange: move |event: FormEvent| {
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
            }
            if photos.is_empty() {
                div { class: "gallery-empty", Icon { name: "plus" } p { "Add photos to this gallery" } }
            } else {
                div { class: "gallery-grid",
                    crate::form_input::FormInput { class: "gallery-sort-result", r#type: "hidden", oninput: move |event: FormEvent| {
                        let value = event.value();
                        let mut parts = value.split(',').filter_map(|part| part.parse::<usize>().ok());
                        let (Some(from), Some(to), None) = (parts.next(), parts.next(), parts.next()) else { return; };
                        if from == to { return; }
                        if let Some(property) = draft.write().as_mut() {
                            if let Some(list) = photos_mut(property, room_id) {
                                if from < list.len() && to < list.len() {
                                    let photo = list.remove(from);
                                    list.insert(to, photo);
                                    message.set("Photo order changed. Save to keep changes.".into());
                                }
                            }
                        }
                    } }
                    for (index, photo) in photos.iter().enumerate() {
                        div { key: "{photo}", class: if index == 0 { "gallery-photo is-cover" } else { "gallery-photo" }, "data-gallery-index": "{index}",
                            button { r#type: "button", class: "gallery-open", title: "Open photo", aria_label: "Open photo {index + 1}", onclick: move |_| viewing.set(Some(index)),
                                img { src: "{photo}", alt: "{title} {index + 1}" }
                            }
                            button { r#type: "button", class: "gallery-remove", title: "Remove photo", aria_label: "Remove photo {index + 1}", onclick: move |_| {
                                if let Some(property) = draft.write().as_mut() { if let Some(list) = photos_mut(property, room_id) { if index < list.len() { list.remove(index); } } }
                                message.set("Photo removed. Save to keep changes.".into());
                            }, Icon { name: "minus" } }
                        }
                    }
                }
            }
            if viewing().is_some() && !photos.is_empty() {
                div { class: "gallery-viewer", role: "dialog", aria_modal: "true", aria_label: "{title} full view",
                    div { class: "gallery-viewer-top",
                        button { r#type: "button", class: "gallery-back back-icon-button", aria_label: "Back to property details", title: "Back to property details", onclick: move |_| viewing.set(None), Icon { name: "arrow-left" } }
                        span { "{photos.len()} photos" }
                    }
                    div { class: "gallery-viewer-scroll",
                        div { class: "gallery-viewer-stack",
                            for (index, photo) in photos.iter().enumerate() {
                                figure { id: "gallery-viewer-photo-{room_id:?}-{index}", class: "gallery-viewer-photo",
                                    img { src: "{photo}", alt: "{title} {index + 1} of {photos.len()}" }
                                    figcaption { if index == 0 { "Cover · " } "{index + 1} of {photos.len()}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
