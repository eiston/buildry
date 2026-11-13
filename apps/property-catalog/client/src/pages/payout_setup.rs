use crate::{
    components::forms::app_form::{AppForm, FormRows},
    components::collapsible_panel::FormCard,
    components::forms::date_picker::DatePicker,
    components::forms::form_button::{ButtonVariant, FormButton},
    components::forms::form_input::{FormCheckbox, FormInput},
    components::forms::form_select::{FormField, FormSelect, FormValue},
};
use dioxus::prelude::*;
use gloo_net::http::Request;
use serde::Deserialize;
use std::collections::BTreeMap;
#[derive(Clone, Copy)]
struct ProfileDefaults(Signal<BTreeMap<String, String>>);

#[derive(Clone, Deserialize, PartialEq)]
struct Setup {
    test_mode: bool,
    #[serde(default)]
    defaults: BTreeMap<String, String>,
    publishable_key: String,
    status: String,
    custom: bool,
    pad_enabled: bool,
    #[serde(default)]
    pad_requested: bool,
    business_type: String,
    requirements: Vec<String>,
    pending: bool,
    bank: Option<String>,
    people: Vec<PersonSummary>,
}

#[derive(Clone, Deserialize, PartialEq)]
struct PersonSummary {
    id: String,
    name: String,
}

#[component]
pub fn PayoutSetup(#[props(default)] verification: bool) -> Element {
    let mut defaults = use_signal(BTreeMap::<String, String>::new);
    use_context_provider(|| ProfileDefaults(defaults));
    let mut state = use_signal(|| None::<Setup>);
    let mut error = use_signal(String::new);
    let mut enabling = use_signal(|| false);
    use_effect(move || {
        spawn(async move {
            match Request::get("/api/stripe/connect/setup").send().await {
                Ok(response) if response.ok() => {
                    if let Ok(value) = response.json::<Setup>().await {
                        defaults.set(value.defaults.clone());
                        state.set(Some(value));
                        return;
                    }
                }
                Ok(response) => {
                    error.set(
                        response
                            .text()
                            .await
                            .unwrap_or_else(|_| "Could not load payout settings.".into()),
                    );
                    return;
                }
                Err(_) => {}
            }
            error.set("Could not load payout settings.".into());
        });
    });
    let Some(current) = state() else {
        return rsx! {
            if error().is_empty() { p { role: "status", class: "payment-history-message", "Loading payout settings…" } }
            else { p { role: "alert", class: "tenant-request-error", "{error}" } }
        };
    };
    rsx! {
        document::Script { src: "/assets/payout-setup.mjs?v=20261106-bills", r#type: "module" }
        div { class: "payout-setup", "data-payout-setup": "", "data-stripe-key": current.publishable_key.clone(), "data-business-type": current.business_type.clone(), "data-profile-defaults":serde_json::to_string(&current.defaults).unwrap_or_default(),
            if current.custom { div { class: "payout-setup-status",
                span { class: if current.status == "ready" { "rent-badge rent-badge-active" } else { "rent-badge" },
                    match current.status.as_str() { "ready" => "Payouts enabled", "new" => "Not connected", _ if current.pending && current.requirements.is_empty() => "Verification pending", _ => "Setup incomplete" }
                }
                if current.test_mode { span { class: "rent-badge", "Test mode" } }
            }
            }
            if !error().is_empty() { p { role: "alert", class: "tenant-request-error", "{error}" } }
            if !current.custom { crate::pages::admin_pages::StripePayouts {} }
            else if verification && current.status!="new" { DetailsForm { current: current.clone() } }
            else { BankForm { current:current.clone() } }
            if !current.pad_requested && current.status!="new" {FormButton {variant:ButtonVariant::Outline,r#type:"button",busy:enabling(),onclick:move|_|{enabling.set(true);spawn(async move {match Request::post("/api/stripe/connect/setup").json(&serde_json::json!({"action":"enable_pad"})).unwrap().send().await {Ok(response) if response.ok()=>{if let Ok(value)=response.json::<Setup>().await {state.set(Some(value));}},Ok(response)=>error.set(response.text().await.unwrap_or_default()),Err(_)=>error.set("Could not enable bank payments.".into())}enabling.set(false);});},"Enable bank payments"}}

        }
    }
}

#[component]
fn BankForm(current: Setup) -> Element {
    let mut editing = use_signal(|| false);
    let mut kind = use_signal(|| current.business_type.clone());
    let mut consent = use_signal(|| false);
    let mut holder_value = use_signal(|| {
        current
            .defaults
            .get("bank.account_holder_name")
            .cloned()
            .unwrap_or_default()
    });
    let company_holder = current
        .defaults
        .get("account.company.name")
        .cloned()
        .unwrap_or_default();
    let individual_holder = current
        .defaults
        .get("bank.account_holder_name")
        .cloned()
        .unwrap_or_default();
    let needs_bank = current.bank.is_none()
        || editing()
        || current
            .requirements
            .iter()
            .any(|key| key == "external_account");
    let needs_terms = current.status == "new"
        || current
            .requirements
            .iter()
            .any(|k| k.starts_with("tos_acceptance."));
    let prefix = if kind() == "company" {
        "account.company."
    } else {
        "account.individual."
    };
    rsx! {
        AppForm {class:if current.status=="new" {"payout-bank-start-form"}else{"payout-details-form"},
            FormCard {title:"Bank account",
                if needs_bank {
                    FormRows {
                        if current.status=="new" {FormSelect {label:"Account type",required:true,value:kind(),placeholder:"Choose account type",options:vec![("individual".into(),"Individual".into()),("company".into(),"Business".into())],onchange:move|v|{holder_value.set(if v=="company"&&!company_holder.is_empty(){company_holder.clone()}else{individual_holder.clone()});kind.set(v)}}}
                        FormInput {r#type:"hidden",name:"business_type",value:kind()}
                        for (path,value) in current.defaults.iter().filter(|(path,_)|path.starts_with(prefix)&&(current.status=="new"||current.requirements.iter().any(|field|path.strip_prefix("account.")==Some(field.as_str())))) {FormInput {key:"{path}",r#type:"hidden",name:path.clone(),value:value.clone()}}
                        FormField {label:"Account holder name",required:true,FormInput {name:"bank.account_holder_name",value:holder_value(),required:true,maxlength:"250",oninput:move|e:FormEvent|holder_value.set(e.value())}}
                        SetupField {path:"bank.institution",field:"institution"}
                        SetupField {path:"bank.transit",field:"transit"}
                        SetupField {path:"bank.account_number",field:"account_number"}
                    }
                } else if let Some(bank)=current.bank.clone() {FormRows {FormValue {label:"Payout account",value:bank}} FormButton {variant:ButtonVariant::Text,r#type:"button",onclick:move|_|editing.set(true),"Update bank account"}}
            }
            p {class:"payout-form-error tenant-request-error",role:"alert",hidden:true}
            if needs_bank {
                if needs_terms {label {class:"payout-consent",FormCheckbox {checked:consent(),onchange:move|e:FormEvent|consent.set(e.checked())}span {"I agree to the " a {href:"https://stripe.com/connect-account/legal/full",target:"_blank",rel:"noopener noreferrer","Stripe Connected Account Agreement"} "."}} FormInput {r#type:"hidden",name:"consent",value:if consent(){"yes"}else{"no"}}}
                FormButton {r#type:"submit",class:"payout-submit",variant:ButtonVariant::Primary,disabled:(current.status=="new"&&kind().is_empty())||(needs_terms&&!consent()),"Save bank account"}
            }
        }
        if current.status=="pending" && current.bank.is_some() {a {class:"payments-setup-link",href:crate::pages::payment_history::scoped_payment_href("/payments/settings/verification"),strong {"Complete verification"}crate::components::icon::Icon {name:"chevron-right"}}}
    }
}

#[derive(Clone, PartialEq)]
struct PersonDraft {
    key: String,
    role: String,
}

// Stripe returns the missing fields after every save, including new verification requirements.
fn person_roots(requirements: &[String]) -> Vec<String> {
    let mut roots: Vec<_> = requirements
        .iter()
        .filter_map(|key| {
            let prefix = key.split('.').next()?;
            (prefix.starts_with("person_") || prefix == "representative")
                .then_some(prefix)
                .filter(|_| {
                    key.split_once('.')
                        .is_some_and(|(_, field)| field_spec(field).is_some())
                })
                .map(str::to_string)
        })
        .collect();
    roots.sort();
    roots.dedup();
    roots
}

fn supported(key: &str) -> bool {
    key == "external_account"
        || key == "documents.proof_of_registration.files"
        || key.starts_with("tos_acceptance.")
        || key == "business_type"
        || matches!(
            key,
            "business_profile.url"
                | "business_profile.product_description"
                | "business_profile.mcc"
                | "business_profile.support_phone"
                | "company.owners_provided"
                | "company.directors_provided"
                | "company.executives_provided"
        )
        || field_spec(key.split_once('.').map(|(_, tail)| tail).unwrap_or(key)).is_some()
}

#[component]
fn DetailsForm(current: Setup) -> Element {
    let mut people = use_signal(Vec::<PersonDraft>::new);
    let mut next_person = use_signal(|| 0u32);
    let mut consent = use_signal(|| false);
    let unknown: Vec<_> = current
        .requirements
        .iter()
        .filter(|key| !supported(key))
        .cloned()
        .collect();
    let personal = normalized_fields(
        current
            .requirements
            .iter()
            .filter_map(|k| k.strip_prefix("individual.").map(str::to_string))
            .collect(),
    );
    let company = normalized_fields(
        current
            .requirements
            .iter()
            .filter_map(|k| {
                k.strip_prefix("company.")
                    .filter(|s| !s.ends_with("_provided"))
                    .map(str::to_string)
            })
            .collect(),
    );
    let initial_persons = person_roots(&current.requirements);
    let needs_terms = current
        .requirements
        .iter()
        .any(|k| k.starts_with("tos_acceptance."));
    let has_fields = current
        .requirements
        .iter()
        .any(|key| key == "documents.proof_of_registration.files")
        || !personal.is_empty()
        || !company.is_empty()
        || !initial_persons.is_empty()
        || needs_terms
        || !people().is_empty();
    rsx! {
        AppForm { class: "payout-details-form".to_string(),
            if !company.is_empty() { FormCard {title:"Business verification",FormRows {for key in normalized_fields(company) {SetupField {key:"{key}",path:format!("account.company.{key}"),field:key}}}} }
            if !personal.is_empty() {
                FormCard { title: "Identity verification".to_string(),
                    FormRows {
                        for key in normalized_fields(personal) { SetupField { key: "{key}", path: format!("account.individual.{key}"), field: key } }
                    }
                }
            }
            for root in initial_persons {
                PersonFields { key: "{root}", root: root.clone(), display_name: current.people.iter().find(|p| p.id == root).map(|p| p.name.clone()), role: if root.starts_with("person_") { String::new() } else { root.trim_end_matches('s').to_string() },
                    fields: normalized_fields(current.requirements.iter().filter_map(|key| key.strip_prefix(&format!("{root}.")).map(str::to_string)).collect()) }
            }
            for person in people() {
                PersonFields { key: "{person.key}", root: person.key.clone(), role: person.role.clone(),
                    fields: added_person_fields(&current.requirements, &person.role) }
                FormButton { variant: ButtonVariant::Text, r#type: "button", onclick: { let key = person.key.clone(); move |_| people.write().retain(|p| p.key != key) }, "Remove person" }
            }
            if current.business_type == "company" {
                div { class: "payout-person-actions",
                    FormButton { variant: ButtonVariant::Outline, r#type: "button", onclick: move |_| { let key = format!("new_{}_{}",js_sys::Date::now() as u64,next_person()); next_person += 1; people.write().push(PersonDraft {key,role:"owner".into()}); }, "Add owner" }
                    FormButton { variant: ButtonVariant::Outline, r#type: "button", onclick: move |_| { let key = format!("new_{}_{}",js_sys::Date::now() as u64,next_person()); next_person += 1; people.write().push(PersonDraft {key,role:"director".into()}); }, "Add director" }
                    FormButton { variant: ButtonVariant::Outline, r#type: "button", onclick: move |_| { let key = format!("new_{}_{}",js_sys::Date::now() as u64,next_person()); next_person += 1; people.write().push(PersonDraft {key,role:"executive".into()}); }, "Add executive" }
                }
                for (key,label) in [("owners_provided","All owners have been added"),("directors_provided","All directors have been added"),("executives_provided","All executives have been added")] {
                    if current.requirements.contains(&format!("company.{key}")) { SetupAcknowledgement { name: format!("account.company.{key}"), label } }
                }
            }
            if current.requirements.iter().any(|key| key == "documents.proof_of_registration.files") {
                FormCard { title: "Business verification".to_string(), FormRows {
                    SetupField { path: "registration_file".to_string(), field: "registration_file".to_string() }
                } }
            }
            if !unknown.is_empty() {
                div { class: "payout-verification",
                    strong { "Additional verification required" }
                    FormButton { r#type: "button", variant: ButtonVariant::Outline, class: "payout-verification-button".to_string(), "Complete verification" }
                }
            }
            if needs_terms {
                label { class: "payout-consent",
                    FormCheckbox { checked: consent(), onchange: move |event: FormEvent| consent.set(event.checked()) }
                    span { "I agree to the " a { href: "https://stripe.com/connect-account/legal/full", target: "_blank", rel: "noopener noreferrer", "Stripe Connected Account Agreement" } "." }
                }
                FormInput { r#type: "hidden", name: "consent", value: if consent() { "yes" } else { "no" } }
            }
            p { class: "payout-form-error tenant-request-error", role: "alert", hidden: true }
            if has_fields {
                FormButton { r#type: "submit", variant: ButtonVariant::Primary, class: "payout-submit".to_string(), disabled: needs_terms && !consent(), "Save verification details" }
            } else if current.pending {
                p { role: "status", class: "payment-history-message", "Stripe is reviewing your details." }
            }
        }
    }
}

#[component]
fn SetupAcknowledgement(name: String, label: &'static str) -> Element {
    let mut checked = use_signal(|| false);
    rsx! { label { class: "payout-consent", FormCheckbox { checked: checked(), onchange: move |e:FormEvent| checked.set(e.checked()) } span { "{label}" } }
        FormInput { r#type: "hidden", name, value: if checked() {"true"} else {"false"} }
    }
}

#[component]
fn PersonFields(
    root: String,
    role: String,
    fields: Vec<String>,
    display_name: Option<String>,
) -> Element {
    let mut also_owner = use_signal(|| false);
    let mut also_director = use_signal(|| false);
    let title = match role.as_str() {
        "representative" => "Account representative",
        "owner" => "Owner",
        "director" => "Director",
        "executive" => "Executive",
        _ => "Verification details",
    };
    let title = display_name
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| title.to_string());
    rsx! { FormCard { title,
        div { "data-payout-person": root.clone(), "data-person-role": role.clone(),
            FormRows { for key in fields { SetupField { key: "{key}", path: format!("person.{root}.{key}"), field: key } } }
            if role == "representative" {
                label { class: "payout-consent", FormCheckbox { checked: also_owner(), onchange: move |e:FormEvent| also_owner.set(e.checked()) } span { "Also an owner" } }
                FormInput { r#type: "hidden", name: format!("person.{root}.relationship.owner"), value: if also_owner() {"true"} else {"false"} }
                if also_owner() { FormRows { SetupField { path: format!("person.{root}.relationship.percent_ownership"), field: "relationship.percent_ownership".to_string() } } }
                label { class: "payout-consent", FormCheckbox { checked: also_director(), onchange: move |e:FormEvent| also_director.set(e.checked()) } span { "Also a director" } }
                FormInput { r#type: "hidden", name: format!("person.{root}.relationship.director"), value: if also_director() {"true"} else {"false"} }
            }

        }
    } }
}

fn added_person_fields(requirements: &[String], role: &str) -> Vec<String> {
    let prefix = format!("{role}s.");
    let mut fields: Vec<_> = requirements
        .iter()
        .filter_map(|key| key.strip_prefix(&prefix).map(str::to_string))
        .collect();
    if fields.is_empty() {
        fields = vec!["first_name".into(), "last_name".into(), "email".into()];
    }
    if role == "owner" {
        fields.push("relationship.percent_ownership".into());
    }
    normalized_fields(fields)
}

fn normalized_fields(fields: Vec<String>) -> Vec<String> {
    let mut result = vec![];
    for field in fields {
        let field = if field.starts_with("dob.") {
            "dob".into()
        } else if matches!(
            field.as_str(),
            "verification.document" | "verification.additional_document"
        ) {
            format!("{field}.front")
        } else {
            field
        };
        if !result.contains(&field) && field_spec(&field).is_some() {
            result.push(field);
        }
    }
    let order = [
        "name",
        "first_name",
        "last_name",
        "email",
        "phone",
        "dob",
        "address.line1",
        "address.line2",
        "address.city",
        "address.state",
        "address.postal_code",
        "tax_id",
        "id_number",
        "relationship.title",
        "relationship.percent_ownership",
    ];
    result.sort_by_key(|field| {
        order
            .iter()
            .position(|key| key == field)
            .unwrap_or(order.len())
    });
    result
}

fn field_spec(field: &str) -> Option<(&'static str, &'static str)> {
    Some(match field {
        "registration_file" => ("Proof of business registration", "file"),
        "verification.document" | "verification.document.front" => ("Identity document", "file"),
        "verification.document.back" => ("Back of identity document", "file"),
        "verification.additional_document" | "verification.additional_document.front" => {
            ("Proof of address", "file")
        }
        "verification.additional_document.back" => ("Back of proof of address", "file"),
        "first_name" => ("Legal first name", "text"),
        "last_name" => ("Legal last name", "text"),
        "name" => ("Legal business name", "text"),
        "email" => ("Email", "email"),
        "phone" => ("Phone number", "tel"),
        "dob" | "dob.day" | "dob.month" | "dob.year" => ("Date of birth", "date"),
        "address.line1" => ("Street address", "text"),
        "address.line2" => ("Apartment / suite", "text"),
        "address.city" => ("City", "text"),
        "address.state" => ("Province / territory", "province"),
        "address.postal_code" => ("Postal code", "text"),
        "tax_id" => ("Business tax ID", "text"),
        "id_number" => ("Social insurance number", "password"),
        "relationship.title" => ("Job title", "text"),
        "relationship.percent_ownership" => ("Ownership percentage", "number"),
        "account_holder_name" => ("Account holder name", "text"),
        "institution" => ("Institution number", "text"),
        "transit" => ("Transit number", "text"),
        "account_number" => ("Account number", "password"),
        "support_phone" => ("Business support phone", "tel"),
        "mcc" => ("Business activity", "activity"),
        "url" => ("Business website", "url"),
        "product_description" => ("Business description", "text"),
        _ => return None,
    })
}

#[component]
fn SetupField(path: String, field: String, #[props(default)] initial: Option<String>) -> Element {
    let default = initial
        .or_else(|| {
            try_use_context::<ProfileDefaults>().and_then(|values| values.0().get(&path).cloned())
        })
        .unwrap_or_default();
    let mut value = use_signal(|| default);
    let Some((label, kind)) = field_spec(&field) else {
        return rsx! {};
    };
    if kind == "file" {
        return rsx! {
            FormField { label, required: true,
                div { "data-payout-document": path,
                    crate::components::forms::upload_dropzone::UploadDropzone { on_files: move |_| {}, disabled: false, label: "Drop document or browse", hint: "JPG, PNG or PDF · 10 MB max", aria_label: label, accept: "image/jpeg,image/png,application/pdf", multiple: false, preserve_selection: true }
                }
            }
        };
    }
    if kind == "date" {
        return rsx! {
            FormField { label, required: true, DatePicker { label: label.to_string(), value: value(), max: js_sys::Date::new_0().to_locale_date_string("en-CA", &js_sys::Object::new()).as_string(), on_change: move |v| value.set(v) } }
            FormInput { r#type: "hidden", name: path, value: value() }
        };
    }
    if kind == "activity" {
        return rsx! {
            FormSelect { label, required: true, value: value(), placeholder: "Choose business activity",
                options: vec![("6513".into(),"Property management & rentals".into())], onchange: move |v| value.set(v) }
            FormInput { r#type: "hidden", name: path, value: value() }
        };
    }
    if kind == "province" {
        return rsx! {
            FormSelect { label, required: true, value: value(), placeholder: "Choose province / territory",
                options: [("AB","Alberta"),("BC","British Columbia"),("MB","Manitoba"),("NB","New Brunswick"),("NL","Newfoundland and Labrador"),("NS","Nova Scotia"),("NT","Northwest Territories"),("NU","Nunavut"),("ON","Ontario"),("PE","Prince Edward Island"),("QC","Quebec"),("SK","Saskatchewan"),("YT","Yukon")].into_iter().map(|(v,l)|(v.into(),l.into())).collect::<Vec<_>>(),
                onchange: move |v| value.set(v)
            }
            FormInput { r#type: "hidden", name: path, value: value() }
        };
    }
    let maxlen = match field.as_str() {
        "institution" => "3",
        "transit" => "5",
        "account_number" => "12",
        _ => "250",
    };
    rsx! { FormField { label, required: true,
        FormInput { r#type: kind, name: path, value: value(), maxlength: maxlen, min: if kind == "number" { Some("0".to_string()) } else { None }, max: if kind == "number" { Some("100".to_string()) } else { None }, step: if kind == "number" { Some("0.01".to_string()) } else { None }, autocomplete: "off", inputmode: if matches!(field.as_str(),"institution"|"transit"|"account_number"|"id_number") { Some("numeric".to_string()) } else { None }, oninput: move |e:FormEvent| value.set(e.value()) }
    } }
}
