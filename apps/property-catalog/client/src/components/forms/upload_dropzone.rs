use dioxus::prelude::*;
use dioxus::html::FileData;

/// Gallery and server-rendered forms use the same browser upload component.
#[component]
pub fn UploadDropzone(
    on_files: EventHandler<Vec<FileData>>,
    #[props(default)] disabled: bool,
    #[props(default = "Drop photos or browse")] label: &'static str,
    #[props(default = "JPG, PNG, WebP · 5 MB each")] hint: &'static str,
    #[props(default = "Upload gallery photos")] aria_label: &'static str,
    #[props(default = "image/jpeg,image/png,image/webp")] accept: &'static str,
    #[props(default = true)] multiple: bool,
    #[props(default)] compact: bool,
    #[props(default)] preserve_selection: bool,
) -> Element {
    let mut picker = use_signal(|| 0_u32);
    rsx! {
        document::Script { src: "/assets/upload-dropzone.mjs?v=20261031", r#type: "module" }
        label { class: if compact {"upload-dropzone compact"}else{"upload-dropzone"}, "data-upload-dropzone": "", "data-upload-label": label, "data-upload-hint": hint,
            input { key: "{picker}", class: "shared-form-file", r#type: "file", aria_label, accept, multiple, disabled,
                onchange: move |event| { on_files.call(event.files()); if !preserve_selection { picker.set(picker() + 1); } }
            }
        }
    }
}
