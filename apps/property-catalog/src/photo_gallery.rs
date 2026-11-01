use crate::{form_button::{ButtonVariant, FormButton}, form_input::FormInput, icon::Icon, shared_ui::Dialog};
use dioxus::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
static NEXT_VIEWER: AtomicUsize = AtomicUsize::new(1);

fn scroll_to_photo(id: usize, selected: usize) {
    if let Some(element) = web_sys::window().and_then(|window| window.document()).and_then(|document| document.get_element_by_id(&format!("photo-viewer-{id}-{selected}"))) { element.scroll_into_view(); }
}

#[component]
pub fn PhotoGallery(
    photos: Vec<String>,
    title: String,
    on_open: EventHandler<usize>,
    #[props(default)] on_remove: Option<EventHandler<usize>>,
    #[props(default)] on_reorder: Option<EventHandler<(usize, usize)>>,
    #[props(default)] cover: bool,
    #[props(default)] disabled: bool,
    #[props(default = "photo")] item_label: &'static str,
) -> Element {
    rsx! { div { class: "gallery-grid",
        if let Some(on_reorder) = on_reorder {
            FormInput { class: "gallery-sort-result", r#type: "hidden", oninput: move |event: FormEvent| {
                let value = event.value();
                let mut values = value.split(',').filter_map(|value| value.parse::<usize>().ok());
                if let (Some(from), Some(to), None) = (values.next(), values.next(), values.next()) {
                    if !disabled && from != to { on_reorder.call((from, to)); }
                }
            } }
        }
        for (index, photo) in photos.iter().enumerate() {
            div { key: "{photo}", class: if cover && index == 0 { "gallery-photo is-cover" } else { "gallery-photo" },
                "data-gallery-index": "{index}",
                FormButton { r#type: "button", class: "gallery-open", aria_label: "Open {item_label} {index + 1}", onclick: move |_| on_open.call(index),
                    img { loading: "lazy", decoding: "async", src: "{photo}", alt: "{title} {index + 1}" }
                }
                if let Some(on_remove) = on_remove {
                    FormButton { r#type: "button", variant: ButtonVariant::Icon, class: "gallery-remove", disabled,
                        aria_label: "Remove {item_label} {index + 1}", onclick: move |_| on_remove.call(index), Icon { name: "minus" }
                    }
                }
            }
        }
    } }
}

#[component]
pub fn PhotoViewer(
    photos: Vec<String>,
    title: String,
    selected: usize,
    on_close: EventHandler<()>,
    #[props(default = "Back to photos".into())] back_label: String,
    #[props(default)] cover: bool,
) -> Element {
    let id = use_hook(|| NEXT_VIEWER.fetch_add(1, Ordering::Relaxed));
    use_effect(move || {
        scroll_to_photo(id, selected);
    });
    rsx! {
        Dialog { label: format!("{title} full view"), on_close, show_close: false, backdrop_class: "gallery-viewer", card_class: "shared-photo-viewer",
            div { class: "gallery-viewer-top",
                FormButton { r#type: "button", variant: ButtonVariant::Icon, class: "gallery-back back-icon-button", aria_label: back_label.clone(), title: back_label,
                    onclick: move |_| on_close.call(()), Icon { name: "arrow-left" }
                }
                span { "{photos.len()} " if photos.len() == 1 { "photo" } else { "photos" } }
            }
            div { class: "gallery-viewer-scroll", div { class: "gallery-viewer-stack",
                for (index, photo) in photos.iter().enumerate() {
                    figure { id: "photo-viewer-{id}-{index}", class: "gallery-viewer-photo",
                        img { loading: if index <= selected { "eager" } else { "lazy" }, decoding: "async", src: "{photo}", alt: "{title} {index + 1} of {photos.len()}",
                            onload: move |_| { if index <= selected { scroll_to_photo(id, selected); } }
                        }
                        figcaption { if cover && index == 0 { "Cover · " } "{index + 1} of {photos.len()}" }
                    }
                }
            } }
        }
    }
}
