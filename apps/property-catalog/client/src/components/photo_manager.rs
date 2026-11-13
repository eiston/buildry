use crate::components::photo_gallery::{PhotoGallery, PhotoViewer};
use crate::{services::api, model::Property};
use dioxus::prelude::*;
use dioxus::html::FileData;

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

#[derive(Clone, PartialEq)]
struct UploadResult { name: String, status: String, failed: bool }

fn upload_files(
    files: Vec<FileData>, mut draft: Signal<Option<Property>>, room_id: Option<u32>,
    mut uploading: Signal<bool>, mut message: Signal<String>,
    mut results: Signal<Vec<UploadResult>>, mut failed: Signal<Vec<FileData>>,
    mut completed: Signal<usize>, mut total: Signal<usize>, on_change: EventHandler<()>,
) {
    if uploading() || files.is_empty() { return; }
    uploading.set(true);
    completed.set(0);
    total.set(files.len());
    results.set(files.iter().map(|file| UploadResult { name: file.name(), status: "Waiting".into(), failed: false }).collect());
    failed.set(Vec::new());
    spawn(async move {
        let mut added = 0;
        for (index, file) in files.into_iter().enumerate() {
            results.write()[index].status = "Uploading…".into();
            match api::upload_image(file.clone()).await {
                Ok(path) => {
                    if let Some(property) = draft.write().as_mut() {
                        if let Some(list) = photos_mut(property, room_id) { list.push(path); added += 1; }
                    }
                    if added > 0 { on_change.call(()); }
                    results.write()[index].status = "Added".into();
                }
                Err(error) => {
                    failed.write().push(file);
                    let mut entries = results.write(); entries[index].status = error; entries[index].failed = true;
                }
            }
            completed.set(index + 1);
        }
        let failures = failed.read().len();
        if failures > 0 { message.set(format!("{added} added · {failures} failed")); }
        uploading.set(false);
    });
}

#[component]
pub fn PhotoManager(
    mut draft: Signal<Option<Property>>,
    room_id: Option<u32>,
    uploading: Signal<bool>,
    mut message: Signal<String>,
    on_change: EventHandler<()>,
) -> Element {
    let mut viewing = use_signal(|| None::<usize>);
    let results = use_signal(Vec::<UploadResult>::new);
    let failed = use_signal(Vec::<FileData>::new);
    let completed = use_signal(|| 0_usize);
    let total = use_signal(|| 0_usize);
    let mut removed = use_signal(|| None::<(usize, String)>);
    let photos = draft.read().as_ref().map(|property| match room_id {
        Some(id) => property.spaces.iter().find(|room| room.id == id).map(|room| room.photos.clone()).unwrap_or_default(),
        None => property.photos.clone(),
    }).unwrap_or_default();
    let title = if room_id.is_some() {
        "Room photos"
    } else {
        "House photos"
    };

    rsx! {
        div { class: "photo-manager",
            crate::components::forms::upload_dropzone::UploadDropzone {
                disabled: uploading(),
                on_files: move |files| upload_files(files, draft, room_id, uploading, message, results, failed, completed, total, on_change)
            }
            if total() > 0 {
                div { class: "upload-results", aria_busy: uploading(),
                    p { role: "status", "{completed} of {total} files processed" }
                    progress { max: "{total}", value: "{completed}", aria_label: "Files processed" }
                    ul { for (index, result) in results().iter().enumerate() {
                        li { key: "{index}", class: if result.failed { "upload-result failed" } else { "upload-result" },
                            strong { "{result.name}" } span { "{result.status}" }
                        }
                    } }
                    if !failed.read().is_empty() && !uploading() {
                        button { r#type: "button", class: "outline-button", onclick: move |_| upload_files(failed(), draft, room_id, uploading, message, results, failed, completed, total, on_change), "Retry failed files" }
                    }
                }
            }
            if let Some((index, photo)) = removed() {
                div { class: "gallery-undo", role: "status", span { "Photo removed." }
                    button { r#type: "button", class: "outline-button", disabled: uploading(), onclick: move |_| {
                        if let Some(property) = draft.write().as_mut() { if let Some(list) = photos_mut(property, room_id) { list.insert(index.min(list.len()), photo.clone()); } }
                        removed.set(None); on_change.call(());
                    }, "Undo" }
                }
            }
            if !photos.is_empty() {
                PhotoGallery { photos: photos.clone(), title, cover: true, disabled: uploading(),
                    on_open: move |index| viewing.set(Some(index)),
                    on_reorder: move |(from, to)| {
                        if let Some(property) = draft.write().as_mut() { if let Some(list) = photos_mut(property, room_id) {
                            if from < list.len() && to < list.len() { let photo = list.remove(from); list.insert(to, photo); }
                        } }
                        on_change.call(());
                    },
                    on_remove: move |index| {
                        if let Some(property) = draft.write().as_mut() { if let Some(list) = photos_mut(property, room_id) {
                            if index < list.len() { removed.set(Some((index, list.remove(index)))); }
                        } }
                        on_change.call(());
                    },
                }
            }
            if let Some(selected) = viewing() { if !photos.is_empty() {
                PhotoViewer { photos, title, selected, cover: true, back_label: "Back to property details", on_close: move |_| viewing.set(None) }
            } }
        }
    }
}
