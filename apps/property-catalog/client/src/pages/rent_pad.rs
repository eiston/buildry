use crate::{
    components::forms::app_form::{AppForm, FormRows},
    components::collapsible_panel::FormCard,
    components::forms::form_button::{ButtonVariant, FormButton},
    components::forms::form_input::FormInput,
    components::forms::form_select::FormField,
};
use dioxus::prelude::*;

#[component]
pub fn RentPad(
    client_secret: String,
    publishable_key: String,
    stripe_account: Option<String>,
    verify: bool,
    on_back: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        document::Script { src: "/assets/rent-pad.mjs?v=20261106-payments", r#type: "module" }
        div { class: "rent-pad-entry rent-checkout", "data-rent-pad": client_secret,
            "data-stripe-key": publishable_key, "data-stripe-account": stripe_account,
            "data-verify": if verify { "true" } else { "false" },
            FormCard { title: if verify { "Verify bank account".to_string() } else { "Bank debit (PAD)".to_string() },
                AppForm {
                    FormRows {
                        if verify {
                            PadField { label: "First deposit (cents)", name: "first", kind: "number", numeric: true }
                            PadField { label: "Second deposit (cents)", name: "second", kind: "number", numeric: true }
                        } else {
                            PadField { label: "Account holder name", name: "name", kind: "text" }
                            PadField { label: "Email", name: "email", kind: "email" }
                            PadField { label: "Institution number", name: "institution", kind: "text", numeric: true }
                            PadField { label: "Transit number", name: "transit", kind: "text", numeric: true }
                            PadField { label: "Account number", name: "account", kind: "password", numeric: true }
                        }
                    }
                    p { class: "rent-checkout-error", role: "alert", hidden: true }
                    div { class: "rent-checkout-actions",
                        FormButton { r#type: "submit", variant: ButtonVariant::Primary, class: "rent-checkout-submit".to_string(), if verify { "Verify bank account" } else { "Authorize PAD" } }
                        FormButton { r#type: "button", variant: ButtonVariant::Text, class: "rent-checkout-back".to_string(), onclick: move |event| on_back.call(event), "Back" }
                    }
                }
            }
        }
    }
}

#[component]
fn PadField(
    label: &'static str,
    name: &'static str,
    kind: &'static str,
    #[props(default)] numeric: bool,
) -> Element {
    let mut value = use_signal(String::new);
    rsx! { FormField { label, required: true,
        FormInput { name, r#type: kind, required: true, value: value(), autocomplete: "off",
            inputmode: if numeric { Some("numeric".to_string()) } else { None },
            min: if kind == "number" { Some("1".to_string()) } else { None }, max: if kind == "number" { Some("99".to_string()) } else { None }, step: if kind == "number" { Some("1".to_string()) } else { None },
            maxlength: match name { "institution" => "3", "transit" => "5", "account" => "12", _ => "250" },
            oninput: move |e:FormEvent| value.set(e.value())
        }
    } }
}
