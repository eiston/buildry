use dioxus::prelude::*;
use wasm_bindgen_futures::JsFuture;
use crate::components::icon::Icon;
use crate::MCP_CONNECTOR_PROMPT;

#[component]
pub(crate) fn CopyMcpPromptButton() -> Element {
    let mut copy_status = use_signal(String::new);
    let copied = copy_status() == "MCP instructions copied";
    rsx! {
        div { class: "mcp-copy-action",
            crate::components::forms::form_button::FormButton {
                r#type: "button",
                variant: crate::components::forms::form_button::ButtonVariant::Icon,
                class: "header-search-action",
                aria_label: "Copy Buildry MCP connection instructions",
                title: "Copy Buildry MCP connection instructions",
                onclick: move |_| {
                    spawn(async move {
                        let result = match web_sys::window() {
                            Some(window) => JsFuture::from(window.navigator().clipboard().write_text(MCP_CONNECTOR_PROMPT)).await,
                            None => Err(wasm_bindgen::JsValue::NULL),
                        };
                        copy_status.set(if result.is_ok() {
                            "MCP instructions copied".into()
                        } else {
                            "Could not copy instructions".into()
                        });
                    });
                },
                if copied { Icon { name: "check" } } else { Icon { name: "copy" } }
            }
            if !copy_status().is_empty() {
                span { class: "mcp-copy-status", role: "status", "{copy_status}" }
            }
        }
    }
}
