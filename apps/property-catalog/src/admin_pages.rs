use crate::form_select::{FormField, FormSelect};
use crate::icon::Icon;
use crate::{api, date_picker::DatePicker, model::Property, tour_model::TourBooking};
use dioxus::prelude::*;
use gloo_net::http::{Request, Response};
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;

fn focus_lease_search() {
    spawn(async {
        gloo_timers::future::TimeoutFuture::new(0).await;
        if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            if let Ok(Some(element)) = document.query_selector(".lease-wizard-search-bar input") {
                if let Ok(input) = element.dyn_into::<web_sys::HtmlElement>() {
                    let _ = input.focus();
                }
            }
        }
    });
}

async fn error(response: Response) -> String {
    response
        .text()
        .await
        .unwrap_or_else(|_| "Request failed.".into())
}

async fn save_template_action(id: i64, body: serde_json::Value) -> Result<i64, String> {
    let response = Request::put(&format!("/api/lease-templates/{id}"))
        .json(&body)
        .map_err(|_| "Could not prepare template.".to_string())?
        .send()
        .await
        .map_err(|_| "Could not save template.".to_string())?;
    if !response.ok() {
        return Err(error(response).await);
    }
    let saved: LeaseTemplate = response
        .json()
        .await
        .map_err(|_| "Template saved, but its response could not be read.".to_string())?;
    Ok(saved.id)
}
async fn upload_template_pdf_action(id: i64, bytes: Vec<u8>) -> Result<(), String> {
    let response = Request::put(&format!("/api/lease-templates/{id}/additional-pdf"))
        .header("Content-Type", "application/pdf")
        .body(bytes)
        .map_err(|_| "Could not prepare the additional PDF.".to_string())?
        .send().await.map_err(|_| "Could not upload the additional PDF.".to_string())?;
    if !response.ok() { return Err(error(response).await); }
    Ok(())
}
async fn save_template_with_pdf(
    id: i64,
    body: serde_json::Value,
    pdf: Option<Vec<u8>>,
    email_landlord: bool,
) -> Result<i64, String> {
    let saved_id = save_template_action(id, body).await?;
    if let Some(bytes) = pdf {
        upload_template_pdf_action(saved_id, bytes).await
            .map_err(|reason| format!("Template saved, but the PDF was not added: {reason}"))?;
    }
    if email_landlord {
        let response = Request::post(&format!("/api/lease-templates/{saved_id}/email"))
            .send().await.map_err(|_| "Template saved, but the email could not be sent.".to_string())?;
        if !response.ok() {
            return Err(format!("Template saved, but the email could not be sent: {}", error(response).await));
        }
    }
    Ok(saved_id)
}

fn show_lease_step(mut step: Signal<u8>, next: u8) {
    step.set(next);
    if let Some(window) = web_sys::window() {
        window.scroll_to_with_x_and_y(0.0, 0.0);
    }
}

#[component]
fn FormToggle(label: &'static str, checked: bool, onchange: EventHandler<bool>) -> Element {
    rsx! { button {r#type:"button",class:if checked{"form-toggle on"}else{"form-toggle"},role:"switch",aria_checked:checked,onclick:move |_|onchange.call(!checked),span {"{label}"} span {class:"form-toggle-track",span {class:"form-toggle-thumb"}}} }
}

#[component]
fn Shell(title: String, active: &'static str, children: Element) -> Element {
    let is_access_detail = active == "/access"
        && web_sys::window()
            .and_then(|window| window.location().pathname().ok())
            .is_some_and(|path| matches!(path.as_str(), "/access/new" | "/access/view"));
    let is_access_search = active == "/access"
        && web_sys::window()
            .and_then(|window| window.location().pathname().ok())
            .is_some_and(|path| path == "/access/search");
    let main_class = if is_access_detail {
        "main-content lease-main access-detail-main"
    } else if is_access_search {
        "main-content lease-main search-page-main"
    } else if active == "/leases" || active == "/access" || title.starts_with("Landlord") {
        "main-content lease-main"
    } else {
        "main-content tenant-main"
    };
    let create_action = match title.as_str() {
        "Leases" => Some(("/leases/new", "Create a lease")),
        "Lease templates" => Some(("/leases/templates/new", "Create template")),
        _ => None,
    };
    rsx! {
        document::Title { "{title} | Buildry" }
        document::Stylesheet { href: crate::CSS }
        if active == "/leases" {
            document::Script { src: "/assets/lease-viewer.mjs?v=20261022b", r#type: "module" }
            document::Script { src: "/assets/lease-section-swipe.mjs", r#type: "module" }
        }
        crate::admin_shell::AdminShell { active, property_count: None, main_class,
            section { class: "page-head", h1 { "{title}" } div { class: "page-actions", if title=="Leases" || title=="Lease templates" || title=="Access" { a { class:"header-search-action", href:if title=="Leases"{"/leases/search"}else if title=="Access"{"/access/search"}else{"/leases/templates/search"}, aria_label:if title=="Leases"{"Search leases"}else if title=="Access"{"Search people"}else{"Search templates"}, title:if title=="Leases"{"Search leases"}else if title=="Access"{"Search people"}else{"Search templates"}, Icon { name:"search" } } } if title=="Access" {a {class:"primary-button page-create-action round-add-action",href:"/access/new",aria_label:"Add person",title:"Add person",Icon {name:"plus"} span {"Add person"}}} if let Some((href,label))=create_action {a {class:"primary-button page-create-action round-add-action",href:"{href}",aria_label:"{label}",title:"{label}",Icon {name:"plus"} span {"{label}"}}} } }
            {children}
        }
    }
}

#[derive(Clone, PartialEq, Deserialize, Default)]
struct User {
    id: i64,
    first_name: String,
    last_name: String,
    email: String,
    phone: String,
    role: String,
    active: bool,
    password_set: bool,
    #[serde(default)]
    google_linked: bool,
    landlord_id: Option<i64>,
    #[serde(default)]
    photo_url: String,
}
impl User {
    fn display_name(&self) -> String {
        format!("{} {}", self.first_name.trim(), self.last_name.trim())
            .trim()
            .to_string()
    }
}
fn account_state(user: &User) -> &'static str {
    if !user.active {
        "Inactive"
    } else if user.password_set || user.google_linked {
        "Active"
    } else {
        "Setup needed"
    }
}

pub fn access_page() -> Element {
    let path = web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .unwrap_or_default();
    let is_search = path == "/access/search";
    let is_detail = path == "/access/view" || path == "/access/new";
    let requested_id = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .and_then(|query| {
            query.trim_start_matches('?').split('&').find_map(|part| {
                part.strip_prefix("user=")
                    .and_then(|id| id.parse::<i64>().ok())
            })
        });
    let mut users = use_signal(Vec::<User>::new);
    let mut landlords = use_signal(Vec::<Landlord>::new);
    let mut profile = use_signal(|| User {
        active: true,
        role: "guest".into(),
        ..User::default()
    });
    let mut search = use_signal(String::new);
    let filter = use_signal(|| "all".to_string());
    let mut message = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut uploading_photo = use_signal(|| false);
    let mut loaded = use_signal(|| false);
    use_effect(move || {
        spawn(async move {
            match Request::get("/api/access/users").send().await {
                Ok(r) if r.ok() => match r.json::<Vec<User>>().await {
                    Ok(v) => {
                        if let Some(chosen) = v.iter().find(|user| Some(user.id) == requested_id) {
                            profile.set(chosen.clone())
                        }
                        users.set(v)
                    }
                    Err(_) => message.set("Could not read accounts.".into()),
                },
                Ok(r) => message.set(error(r).await),
                Err(_) => message.set("Could not load accounts.".into()),
            }
            if let Ok(r) = Request::get("/api/landlords").send().await {
                if let Ok(v) = r.json().await {
                    landlords.set(v)
                }
            }
            loaded.set(true);
        });
    });
    let p = profile();
    let password_state = if p.password_set { "set" } else { "not set" };
    let google_state = if p.google_linked { "connected" } else { "not connected" };
    let all = users();
    let query = search().to_lowercase();
    let role_filter = filter();
    let visible: Vec<_> = all
        .iter()
        .filter(|user| {
            (role_filter == "all" || user.role == role_filter)
                && (query.is_empty()
                    || format!(
                        "{} {} {} {}",
                        user.first_name, user.last_name, user.email, user.phone
                    )
                    .to_lowercase()
                    .contains(&query))
        })
        .cloned()
        .collect();
    rsx! { Shell { title: if is_detail { if path == "/access/new" { "Add person" } else { "Person details" } } else { "Access" }, active: "/access",
        if !message().is_empty() { p { class: "availability-message", role: "status", "{message}" } }
        if !is_detail {
            if is_search { crate::searchable_list::SearchScreen {back_href:"/access",placeholder:"Search name, email or phone",value:search(),count:visible.len(),on_search:move |value|search.set(value),
                AccessPeopleList { users: visible.clone(), filter }
            } } else { AccessPeopleList { users: visible.clone(), filter } }
        }
        if path=="/access/view" && !loaded() {div {class:"access-detail-loading",role:"status",aria_label:"Loading person",div {class:"access-loading-bar"} div {class:"access-loading-card",span {} span {} span {} span {}}}}
        if path=="/access/view" && loaded() && p.id==0 {p {class:"lease-list-empty","Person not found. Return to Access and choose a person."} a {class:"outline-button",href:"/access","Back to people"}}
        if is_detail && (path=="/access/new" || (loaded() && p.id>0)) {
            div {class:"access-detail-page",
            crate::form_action_bar::FormActionBar {back_href:"/access".to_string(),back_label:"Back to people".to_string(),title:Some(if p.id==0{"Add a person".to_string()}else{p.display_name()}),
                button {class:"property-save-button",r#type:"submit",form:if p.role=="landlord"{"landlord-editor-form"}else{"access-editor-form"},aria_label:"Save person",title:"Save person",disabled:busy()||uploading_photo()||(p.role!="landlord"&&(p.first_name.trim().is_empty()||p.last_name.trim().is_empty()||p.email.trim().is_empty())),Icon {name:"check"}}
            }
            div {class:"access-detail-content",
            section {class:"access-detail",aria_label:"Account details",
                div {class:"access-detail-head",span {if p.id==0{"NEW PERSON"}else{"ACCOUNT · #{p.id}"}} h2 {if p.id==0{"Add a person"}else{"{p.display_name()}"}}}
                crate::app_form::AppForm {id:"access-editor-form",class:"access-form",onsubmit:move |event: FormEvent|{event.prevent_default();if busy()||uploading_photo(){return}let user=profile();if user.role=="landlord"{return}let id=user.id;let body=serde_json::json!({"first_name":user.first_name,"last_name":user.last_name,"email":user.email,"phone":user.phone,"role":user.role,"active":user.active,"landlord_id":None::<i64>,"photo_url":user.photo_url});busy.set(true);message.set(String::new());spawn(async move{let request=if id==0{Request::post("/api/access/users")}else{Request::put(&format!("/api/access/users/{id}"))};match request.json(&body).unwrap().send().await {Ok(r) if r.ok()=>{let new_id=if id==0{r.json::<User>().await.ok().map(|user|user.id).unwrap_or(0)}else{id};if id==0&&new_id>0 {let _=web_sys::window().map(|w|w.location().set_href(&format!("/access/view?user={new_id}")));}else{if let Ok(r)=Request::get("/api/access/users").send().await {if let Ok(v)=r.json::<Vec<User>>().await {if let Some(saved)=v.iter().find(|user|user.id==new_id){profile.set(saved.clone())}users.set(v)}}message.set("Account saved.".into())}},Ok(r)=>message.set(error(r).await),Err(_)=>message.set("Could not save account.".into())}busy.set(false);});},
                    if p.role!="landlord" {
                        div { class: "access-photo-editor",
                            span { class: "access-photo-avatar", if p.photo_url.is_empty() { "{prospect_initials(&p.display_name())}" } else { img { src: "{p.photo_url}", alt: "Profile photo for {p.display_name()}" } } }
                            div {
                                strong { "Profile photo" }
                                small { "Shown in people and task filters" }
                                label { class: "access-photo-upload", "Upload photo"
                                    crate::form_input::FormFileInput { accept: "image/jpeg,image/png,image/webp", disabled: uploading_photo(), onchange: move |event: FormEvent| {
                                        let files = event.files(); let Some(file) = files.into_iter().next() else { return };
                                        uploading_photo.set(true);
                                        spawn(async move { match crate::api::upload_image(file).await { Ok(path) => profile.write().photo_url = path, Err(err) => message.set(err) } uploading_photo.set(false); });
                                    } }
                                }
                                if !p.photo_url.is_empty() { button { r#type: "button", onclick: move |_| profile.write().photo_url.clear(), "Remove" } }
                            }
                        }
                    }
                    if p.role!="landlord" {div {class:"access-section-label","PERSON"}
                    div {class:"access-fields",label {"First name *",crate::form_input::FormInput {required:true,maxlength:"60",autocomplete:"given-name",value:"{p.first_name}",oninput:move |e: FormEvent|profile.write().first_name=e.value()}} label {"Last name *",crate::form_input::FormInput {required:true,maxlength:"60",autocomplete:"family-name",value:"{p.last_name}",oninput:move |e: FormEvent|profile.write().last_name=e.value()}} label {"Email address *",crate::form_input::FormInput {r#type:"email",required:true,maxlength:"254",value:"{p.email}",oninput:move |e: FormEvent|profile.write().email=e.value()}} label {"Phone number",crate::form_input::FormInput {r#type:"tel",maxlength:"40",value:"{p.phone}",oninput:move |e: FormEvent|profile.write().phone=e.value()}}}}
                    div {class:"access-section-label","ACCESS"}
                    div {class:"access-fields",
                        FormSelect {label:"Role",required:true,value:p.role.clone(),placeholder:"Choose a role",options:vec![("guest".into(),"Guest · browse and book".into()),("tenant".into(),"Tenant · view assigned home".into()),("landlord".into(),"Landlord · view owned properties".into()),("staff".into(),"Staff · manage operations".into()),("admin".into(),"Admin · manage everything".into())],onchange:move |role:String|{let mut user=profile.write();user.role=role.clone();if role!="landlord"{user.landlord_id=None}}}
                        if p.role=="landlord" && p.landlord_id.is_none() {FormSelect {label:"Existing landlord profile",required:false,value:"0".to_string(),placeholder:"Create a profile below",options:landlords().iter().map(|landlord|(landlord.id.to_string(),landlord.legal_name.clone())).collect::<Vec<_>>(),onchange:move |value:String|profile.write().landlord_id=value.parse::<i64>().ok().filter(|id|*id>0)}}
                    }
                }
            }
            }
            if p.id>0 { section {class:"access-detail access-login-methods",aria_label:"Sign-in methods",
                div {class:"access-section-label","SIGN-IN METHODS"}
                p { "Password: {password_state}" }
                p { "Google: {google_state}" }
                p {class:"access-setup-note", if p.google_linked {"Google sign-in is ready. A password email adds password sign-in for this person."} else if p.password_set {"This person can sign in with a password, then connect a Google account with the same email."} else {"Send a setup email to the saved account address. After setting a password, the person can connect Google with the same email."}}
                button {class:"outline-button",r#type:"button",disabled:busy() || !p.active || all.iter().find(|user|user.id==p.id).is_none_or(|saved|saved.email!=p.email),onclick:move |_|{let id=profile().id;busy.set(true);message.set(String::new());spawn(async move{match Request::post(&format!("/api/access/users/{id}/invite")).send().await{Ok(reply) if reply.ok()=>message.set("Password email sent to the saved account address.".into()),Ok(reply)=>message.set(error(reply).await),Err(_)=>message.set("Could not send the password email.".into())}busy.set(false);});},if p.password_set {"Send password reset email"}else{"Send password setup email"}}
            } }
            if p.role=="landlord" { section {class:"access-landlord-section",LandlordSettings {key:"{p.landlord_id.unwrap_or(0)}", landlord_id:p.landlord_id.unwrap_or(0), user_id:p.id, user_first_name:p.first_name.clone(), user_last_name:p.last_name.clone(), user_email:p.email.clone(), user_phone:p.phone.clone(), user_active:p.active } } }
            }
        }
    } }
}

#[component]
fn AccessPeopleList(users: Vec<User>, filter: Signal<String>) -> Element {
    rsx! { crate::searchable_list::JointList {class:"access-register",filters:Some(rsx! {
        div {class:"searchable-list-filters lease-register-filters",role:"group",aria_label:"Filter people",
            for (role,label) in [("all","All"),("guest","Guests"),("tenant","Tenants"),("landlord","Landlords"),("staff","Staff"),("admin","Admins")] {
                button {key:"{role}",r#type:"button",class:if filter()==role{"active"}else{""},onclick:move |_|{let mut filter=filter;filter.set(role.to_string())},"{label}"}
            }
        }
    }),
        if users.is_empty(){p {class:"lease-list-empty","No people match this view."}}
        for (role,label) in [("admin","Admins"),("staff","Staff"),("landlord","Landlords"),("tenant","Tenants"),("guest","Guests")] {
            if users.iter().any(|user|user.role==role) { section {class:"access-role-group",h2 {"{label}"}
                for user in users.iter().filter(|user|user.role==role) {a {key:"{user.id}",class:"access-person register-row",href:"/access/view?user={user.id}",span {class:"lease-prospect-avatar",if user.photo_url.is_empty(){"{prospect_initials(&user.display_name())}"}else{img {src:"{user.photo_url}",alt:""}}} span {class:"access-person-copy",strong {"{user.display_name()}"} small {"{user.email}"}} span {class:if user.active{"access-state"}else{"access-state inactive"},"{account_state(user)}"} Icon {name:"arrow-right"}}}
            } }
        }
    } }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
struct Landlord {
    id: i64,
    legal_name: String,
    service_address: String,
    email: String,
    phone: String,
    signer_name: String,
    signer_title: String,
    signature_authorized: bool,
    property_ids: Vec<i64>,
    #[serde(default)]
    photo_url: String,
}

async fn save_landlord_and_account(
    landlord: Landlord,
    user_id: i64,
    active: bool,
) -> Result<i64, String> {
    let request = if landlord.id == 0 {
        Request::post("/api/landlords")
    } else {
        Request::put(&format!("/api/landlords/{}", landlord.id))
    };
    let response = request
        .json(&landlord)
        .map_err(|_| "Could not prepare landlord details.".to_string())?
        .send()
        .await
        .map_err(|_| "Could not save landlord details.".to_string())?;
    if !response.ok() {
        return Err(error(response).await);
    }
    let saved = response
        .json::<Landlord>()
        .await
        .map_err(|_| "Landlord details saved, but the response could not be read.".to_string())?;
    let (first_name, last_name) = saved
        .legal_name
        .split_once(' ')
        .unwrap_or((saved.legal_name.as_str(), ""));
    let account = serde_json::json!({
        "first_name": first_name,
        "last_name": last_name,
        "email": saved.email,
        "phone": saved.phone,
        "role": "landlord",
        "active": active,
        "landlord_id": saved.id,
    });
    let request = if user_id == 0 {
        Request::post("/api/access/users")
    } else {
        Request::put(&format!("/api/access/users/{user_id}"))
    };
    let response = request
        .json(&account)
        .map_err(|_| "Landlord details saved, but the account could not be prepared.".to_string())?
        .send()
        .await
        .map_err(|_| "Landlord details saved, but the account could not be updated.".to_string())?;
    if !response.ok() {
        return Err(format!(
            "Landlord details saved, but the account could not be updated: {}",
            error(response).await
        ));
    }
    if user_id > 0 {
        Ok(user_id)
    } else {
        response
            .json::<User>()
            .await
            .map(|user| user.id)
            .map_err(|_| {
                "Landlord and account saved, but the new account could not be opened.".to_string()
            })
    }
}

fn property_count_label(count: usize) -> String {
    format!(
        "{count} {}",
        if count == 1 { "property" } else { "properties" }
    )
}
#[component]
pub fn LandlordSettings(
    landlord_id: i64,
    user_id: i64,
    user_first_name: String,
    user_last_name: String,
    user_email: String,
    user_phone: String,
    user_active: bool,
) -> Element {
    let edit_id = landlord_id;
    let mut profile = use_signal(|| Landlord {
        email: user_email.clone(),
        phone: user_phone.clone(),
        ..Landlord::default()
    });
    let mut first_name = use_signal(|| user_first_name.clone());
    let mut last_name = use_signal(|| user_last_name.clone());
    let mut address_mode = use_signal(|| false);
    let mut landlords = use_signal(Vec::<Landlord>::new);
    let mut properties = use_signal(Vec::<Property>::new);
    let mut message = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut property_query = use_signal(String::new);
    let mut property_scope = use_signal(|| "all".to_string());
    use_effect(move || {
        spawn(async move {
            if let Ok(r) = Request::get("/api/landlords").send().await {
                if let Ok(v) = r.json::<Vec<Landlord>>().await {
                    if edit_id > 0 {
                        if let Some(selected) = v.iter().find(|landlord| landlord.id == edit_id) {
                            let (first, last) = selected
                                .legal_name
                                .split_once(' ')
                                .unwrap_or((selected.legal_name.as_str(), ""));
                            first_name.set(first.to_string());
                            last_name.set(last.to_string());
                            profile.set(selected.clone())
                        }
                    }
                    landlords.set(v)
                }
            }
            if let Ok(v) = api::list().await {
                properties.set(v)
            }
        });
    });
    let p = profile();
    let assigned_count = p.property_ids.len();
    let query = property_query().to_lowercase();
    let mut visible_properties: Vec<_> = properties()
        .into_iter()
        .filter(|property| {
            (property_scope() == "all" || p.property_ids.contains(&(property.id as i64)))
                && (query.is_empty()
                    || format!("{} {}", property.name, property.address)
                        .to_lowercase()
                        .contains(&query))
        })
        .collect();
    visible_properties.sort_by(|a, b| {
        p.property_ids
            .contains(&(b.id as i64))
            .cmp(&p.property_ids.contains(&(a.id as i64)))
            .then(a.name.cmp(&b.name))
    });
    let visible_property_count = visible_properties.len();
    rsx! {
        if address_mode() {
            crate::address_picker::AddressSearchScreen { initial_value: p.service_address.clone(), overlay: true,
                onback: move |_| address_mode.set(false),
                onselect: move |selection: crate::address_picker::AddressSelection| {
                    profile.write().service_address = selection.label;
                    address_mode.set(false);
                }
            }
        }
        div {class:"lease-config-section",
        if !message().is_empty() { p { class: "availability-message", role: "status", "{message}" } }
        div {class:"landlord-workspace",
            section {class:"landlord-detail",aria_label:"Landlord details",
                crate::app_form::AppForm {id:"landlord-editor-form",class:"landlord-form",onsubmit:move |event: FormEvent|{event.prevent_default();if busy(){return}let mut landlord=profile();let first=first_name().trim().to_string();let last=last_name().trim().to_string();if first.is_empty()||last.is_empty()||landlord.email.trim().is_empty()||landlord.service_address.trim().is_empty(){message.set("Complete the legal contact details before saving.".into());return}landlord.legal_name=format!("{first} {last}");landlord.signer_name=landlord.legal_name.clone();landlord.signer_title="Owner".into();landlord.signature_authorized=true;busy.set(true);message.set(String::new());spawn(async move {match save_landlord_and_account(landlord,user_id,user_active).await {Ok(id)=>{let _=web_sys::window().map(|w|w.location().set_href(&format!("/access/view?user={id}")));},Err(reason)=>message.set(reason)}busy.set(false);});},
                    h3 {class:"access-section-label","LEGAL CONTACT"}
                    div {class:"landlord-fields",
                        label {"First name *",crate::form_input::FormInput {required:true,value:"{first_name}",autocomplete:"given-name",oninput:move |e: FormEvent|first_name.set(e.value())}}
                        label {"Last name *",crate::form_input::FormInput {required:true,value:"{last_name}",autocomplete:"family-name",oninput:move |e: FormEvent|last_name.set(e.value())}}
                        crate::address_picker::AddressField { label: "Address for notices *", value: p.service_address.clone(), onopen: move |_| address_mode.set(true) }
                        label {"Email address *",crate::form_input::FormInput {r#type:"email",required:true,maxlength:"254",value:"{p.email}",oninput:move |e: FormEvent|profile.write().email=e.value()}}
                        label {"Phone number",crate::form_input::FormInput {r#type:"tel",value:"{p.phone}",oninput:move |e: FormEvent|profile.write().phone=e.value()}}
                    }
                    h3 {class:"access-section-label landlord-properties-heading","PROPERTIES"}
                    div {class:"landlord-property-tools",crate::form_input::FormInput {r#type:"search",placeholder:"Search properties",aria_label:"Search properties",value:"{property_query}",oninput:move |e: FormEvent|property_query.set(e.value())} div {role:"group",aria_label:"Filter properties",button {r#type:"button",class:if property_scope()=="all"{"active"}else{""},onclick:move |_|property_scope.set("all".into()),"All"} button {r#type:"button",class:if property_scope()=="assigned"{"active"}else{""},onclick:move |_|property_scope.set("assigned".into()),"Assigned · {assigned_count}"}}}
                    div {class:"landlord-property-count","{visible_property_count} shown · {property_count_label(assigned_count)} selected"}
                    div {class:"landlord-property-grid lease-wizard-options",if visible_properties.is_empty(){p {class:"landlord-property-empty","No properties match this view."}} for property in visible_properties {crate::form_action_bar::PropertyChoice {key:"{property.id}",image:property.photos.first().cloned().unwrap_or_else(||crate::guest_pages::house_photo(property.id)),name:property.name.clone(),subtitle:landlords().iter().find(|owner|owner.id!=p.id&&owner.property_ids.contains(&(property.id as i64))).map(|owner|format!("Currently {}",owner.legal_name)).unwrap_or_else(||property.location_label()),selected:p.property_ids.contains(&(property.id as i64)),on_select:{let id=property.id as i64;move |_|{let mut value=profile.write();if value.property_ids.contains(&id){value.property_ids.retain(|item|*item!=id)}else{value.property_ids.push(id)}}}}}}
                }
            }
        }
    } }
}

pub fn landlords_page() -> Element {
    use_effect(move || {
        spawn(async move {
            let path = web_sys::window()
                .and_then(|w| w.location().pathname().ok())
                .unwrap_or_default();
            let target = if path.ends_with("/new") {
                "/access/new".to_string()
            } else {
                let landlord_id = web_sys::window()
                    .and_then(|w| w.location().search().ok())
                    .and_then(|query| {
                        query.trim_start_matches('?').split('&').find_map(|part| {
                            part.strip_prefix("id=")
                                .and_then(|id| id.parse::<i64>().ok())
                        })
                    });
                let user_id = if let Ok(reply) = Request::get("/api/access/users").send().await {
                    reply.json::<Vec<User>>().await.ok().and_then(|users| {
                        users
                            .into_iter()
                            .find(|user| user.landlord_id == landlord_id && landlord_id.is_some())
                            .map(|user| user.id)
                    })
                } else {
                    None
                };
                user_id
                    .map(|id| format!("/access/view?user={id}"))
                    .unwrap_or_else(|| "/access".into())
            };
            let _ = web_sys::window().map(|window| window.location().set_href(&target));
        });
    });
    rsx! { "Opening Access…" }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
struct Details {
    start_date: String,
    end_date: String,
    term_type: String,
    other_term: String,
    monthly_rent: String,
    rent_due_day: String,
    rent_payment_method: String,
    parking: String,
    condo: String,
    services: String,
    utilities: String,
    rent_deposit: String,
    key_deposit: String,
    smoking: String,
    insurance: String,
    additional_terms: String,
    unit_description: String,
    occupants: String,
    notes: String,
    email_notices: bool,
    #[serde(default)]
    landlord_notice_email: String,
    #[serde(default)]
    landlord_contact_details: String,
    gas_included: bool,
    ac_included: bool,
    storage_included: bool,
    laundry_included: bool,
    guest_parking_included: bool,
    laundry_fee: String,
    electricity_payer: String,
    heat_payer: String,
    water_payer: String,
    rent_discount: String,
    partial_rent: String,
    nsf_fee: String,
    key_description: String,
}
#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
struct LeaseTemplate {
    id: i64,
    name: String,
    property_id: Option<i64>,
    room_id: i64,
    lease_type: String,
    landlord_id: Option<i64>,
    details: Details,
    landlord_signed_at: Option<String>,
    landlord_signature: Option<String>,
    #[serde(default)]
    has_additional_pdf: bool,
}
#[derive(Clone, PartialEq, Deserialize)]
struct Lease {
    id: i64,
    tenant_name: String,
    tenant_email: String,
    property_id: i64,
    room_id: i64,
    lease_type: String,
    tour_id: Option<i64>,
    template_id: Option<i64>,
    is_demo: bool,
    details: Details,
    status: String,
    signed_at: Option<String>,
    landlord_signed_at: Option<String>,
    confirmed_at: Option<String>,
    copy_sent_at: Option<String>,
    #[serde(default)]
    photo_id_uploaded: bool,
    #[serde(default)]
    photo_id_verified: bool,
    #[serde(default)]
    photo_id_type: Option<String>,
    #[serde(default)]
    tenancy_ended_at: Option<String>,
}
#[derive(Clone, PartialEq, Deserialize)]
struct EmailStatus {
    configured: bool,
}

fn specified(value: &str) -> String {
    if value.trim().is_empty() {
        "Not specified".into()
    } else {
        value.to_string()
    }
}

#[component]
fn LeaseStoryRow(label: &'static str, value: String) -> Element {
    rsx! { div { class: "lease-story-row", span { "{label}" } strong { "{value}" } } }
}

#[component]
fn LeaseAmenity(label: &'static str, included: bool) -> Element {
    rsx! { li { class: if included { "lease-amenity" } else { "lease-amenity excluded" },
        span { class: "lease-amenity-mark", aria_hidden: "true", if included { "✓" } else { "—" } }
        span { "{label}" } small { if included { "Included" } else { "Not included" } }
    } }
}

#[component]
fn AdminLeaseDetails(
    lease: Lease,
    property_name: String,
    property_address: String,
    room_name: String,
    landlord_name: String,
) -> Element {
    let d = &lease.details;
    let end = if d.term_type == "month-to-month" {
        "Continues monthly".into()
    } else {
        date_label(&d.end_date)
    };
    let term = match d.term_type.as_str() {
        "fixed" => "Fixed term",
        "month-to-month" => "Month to month",
        _ => "Other term",
    };
    rsx! { crate::collapsible_panel::CollapsiblePanel { title: "Lease details", subtitle: Some("Home, dates, payments and terms".into()), badge: None, class: "lease-details", initially_open: false,
        div { class: "lease-story-hero", div { p { class: "lease-story-kicker", "The home" } h3 { "{room_name}" } p { class: "lease-story-address", "{property_address}" } if !d.unit_description.trim().is_empty() && d.unit_description != room_name { p { class: "lease-story-description", "{d.unit_description}" } } }
            div { class: "lease-story-price", span { "Monthly rent" } strong { "C${d.monthly_rent}" } small { "per month" } }
        }
        div { class: "lease-story-timeline", div { class: "tour-detail-schedule", span { class: "tour-detail-label", "Move in" } strong { class: "tour-detail-time", "{date_label(&d.start_date)}" } } div { class: "tour-detail-schedule", span { class: "tour-detail-label", if d.term_type == "month-to-month" { "Term" } else { "Term ends" } } strong { class: "tour-detail-time", "{end}" } } span { class: "lease-story-term", "{term}" } }
        section { class: "lease-story-section", div { class: "lease-story-section-head", span { "01" } div { h3 { "Rent & payments" } } }
            div { class: "lease-story-list",
                LeaseStoryRow { label: "Rent due", value: if d.rent_due_day.is_empty() { "Not specified".into() } else { format!("Day {} of each month", d.rent_due_day) } }
                LeaseStoryRow { label: "Payment method", value: specified(&d.rent_payment_method) }
                LeaseStoryRow { label: "Rent deposit", value: if d.rent_deposit.is_empty() { "None".into() } else { format!("C${}", d.rent_deposit) } }
                LeaseStoryRow { label: "Key deposit", value: if d.key_deposit.is_empty() || d.key_deposit == "0" { "None".into() } else { format!("C${}", d.key_deposit) } }
                if !d.key_description.is_empty() { LeaseStoryRow { label: "Keys and devices", value: d.key_description.clone() } }
                if !d.partial_rent.is_empty() { LeaseStoryRow { label: "Partial rent", value: d.partial_rent.clone() } }
                if !d.rent_discount.is_empty() { LeaseStoryRow { label: "Rent discount", value: d.rent_discount.clone() } }
                if !d.nsf_fee.is_empty() { LeaseStoryRow { label: "NSF charge", value: format!("C${}", d.nsf_fee) } }
                if !d.other_term.is_empty() { LeaseStoryRow { label: "Other term", value: d.other_term.clone() } }
            }
        }
        section { class: "lease-story-section", div { class: "lease-story-section-head", span { "02" } div { h3 { "What the home includes" } } }
            if !d.services.is_empty() { div { class: "lease-story-note", strong { "Services" } p { "{d.services}" } } }
            ul { class: "lease-amenity-list",
                LeaseAmenity { label: "Gas", included: d.gas_included }
                LeaseAmenity { label: "Air conditioning", included: d.ac_included }
                LeaseAmenity { label: "Storage", included: d.storage_included }
                LeaseAmenity { label: "Laundry", included: d.laundry_included }
                LeaseAmenity { label: "Guest parking", included: d.guest_parking_included }
            }
            h4 { class: "lease-story-subhead", "Utilities" }
            div { class: "lease-story-list",
                LeaseStoryRow { label: "Electricity", value: format!("Paid by {}", specified(&d.electricity_payer)) }
                LeaseStoryRow { label: "Heat", value: format!("Paid by {}", specified(&d.heat_payer)) }
                LeaseStoryRow { label: "Water", value: format!("Paid by {}", specified(&d.water_payer)) }
                if !d.laundry_fee.is_empty() { LeaseStoryRow { label: "Laundry fee", value: d.laundry_fee.clone() } }
            }
            if !d.utilities.is_empty() { div { class: "lease-story-note", strong { "Utility details" } p { "{d.utilities}" } } }
        }
        section { class: "lease-story-section", div { class: "lease-story-section-head", span { "03" } div { h3 { "House rules & terms" } } }
            div { class: "lease-story-list",
                LeaseStoryRow { label: "Parking", value: specified(&d.parking) }
                LeaseStoryRow { label: "Condominium", value: specified(&d.condo) }
                LeaseStoryRow { label: "Smoking rules", value: specified(&d.smoking) }
                LeaseStoryRow { label: "Tenant insurance", value: specified(&d.insurance) }
                LeaseStoryRow { label: "Email notices", value: if d.email_notices { "Agreed".to_string() } else { "Not agreed".to_string() } }
                if d.email_notices && !d.landlord_notice_email.is_empty() { LeaseStoryRow { label: "Landlord notice email", value: d.landlord_notice_email.clone() } }
                if d.email_notices && !d.landlord_contact_details.is_empty() { LeaseStoryRow { label: "Landlord contact details", value: d.landlord_contact_details.clone() } }
                if !d.occupants.is_empty() { LeaseStoryRow { label: "Other occupants", value: d.occupants.clone() } }
            }
            if !d.additional_terms.is_empty() { div { class: "lease-story-note", strong { "Additional terms" } p { "{d.additional_terms}" } } }
            if !d.notes.is_empty() { div { class: "lease-story-note", strong { "Notes" } p { "{d.notes}" } } }
        }
        section { class: "lease-story-section", div { class: "lease-story-section-head", span { "04" } div { h3 { "People & contact" } } }
            div { class: "lease-people",
                div { class: "lease-person", span { class: "lease-person-avatar", aria_hidden: "true", "L" } div { small { "Landlord" } strong { "{landlord_name}" } p { "{property_name}" } } }
                div { class: "lease-person", span { class: "lease-person-avatar", aria_hidden: "true", "T" } div { small { "Tenant" } strong { "{lease.tenant_name}" } p { "{lease.tenant_email}" } } }
            }
            div { class: "lease-story-list", LeaseStoryRow { label: "Rental address", value: specified(&property_address) } }
        }
    } }
}

fn date_label(date: &str) -> String {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|day| day.format("%b %-d, %Y").to_string())
        .unwrap_or_else(|_| date.to_string())
}

fn lease_status(lease: &Lease) -> &'static str {
    match lease.status.as_str() {
        "draft" => "Needs sending",
        "sent" => "Awaiting tenant",
        "signed" if lease.landlord_signed_at.is_some() => "Both signed",
        "signed" => "Awaiting landlord",
        "confirmed" => "Confirmed",
        _ => "Tour booked",
    }
}
fn lease_property_name(lease: &Lease, houses: &[Property]) -> String {
    houses
        .iter()
        .find(|house| house.id as i64 == lease.property_id)
        .map(|house| house.name.clone())
        .unwrap_or_else(|| "Property".into())
}
fn lease_overlaps_stay(
    lease: &Lease,
    property_id: i64,
    room_id: i64,
    start: chrono::NaiveDate,
    end: chrono::NaiveDate,
) -> bool {
    if lease.is_demo
        || !matches!(lease.status.as_str(), "signed" | "confirmed")
        || lease.property_id != property_id
        || !(lease.room_id == room_id || lease.room_id == 0 || room_id == 0)
    {
        return false;
    }
    let Ok(lease_start) = chrono::NaiveDate::parse_from_str(&lease.details.start_date, "%Y-%m-%d")
    else {
        return false;
    };
    let lease_end = if lease.details.end_date.is_empty() {
        chrono::NaiveDate::MAX
    } else {
        chrono::NaiveDate::parse_from_str(&lease.details.end_date, "%Y-%m-%d")
            .unwrap_or(chrono::NaiveDate::MAX)
    };
    lease_start <= end && lease_end >= start
}
fn template_landlord_name(owners: &[Landlord], landlord_id: Option<i64>) -> String {
    owners
        .iter()
        .find(|owner| Some(owner.id) == landlord_id)
        .map(|owner| owner.legal_name.clone())
        .unwrap_or_else(|| "Landlord".into())
}
fn tour_upcoming(tour: &TourBooking, today: &str, now: &str) -> bool {
    tour.tour_date.as_str() > today
        || (tour.tour_date.as_str() == today && tour.slot_start.as_str() >= now)
}
#[derive(Clone)]
struct Prospect {
    key: String,
    name: String,
    email: String,
    phone: String,
    tours: Vec<TourBooking>,
}
fn prospect_key(tour: &TourBooking) -> String {
    let phone: String = tour
        .guest_phone
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    if !phone.is_empty() {
        format!("phone:{phone}")
    } else if !tour.guest_email.is_empty() {
        format!("email:{}", tour.guest_email.to_lowercase())
    } else {
        format!("tour:{}", tour.id)
    }
}
fn prospects_from_tours(tours: &[TourBooking]) -> Vec<Prospect> {
    let mut people = Vec::<Prospect>::new();
    for tour in tours {
        let key = prospect_key(tour);
        if let Some(person) = people.iter_mut().find(|person| person.key == key) {
            person.tours.push(tour.clone())
        } else {
            people.push(Prospect {
                key,
                name: tour.guest_name.clone(),
                email: tour.guest_email.clone(),
                phone: tour.guest_phone.clone(),
                tours: vec![tour.clone()],
            })
        }
    }
    for person in &mut people {
        person.tours.sort_by(|a, b| {
            b.tour_date
                .cmp(&a.tour_date)
                .then(b.slot_start.cmp(&a.slot_start))
        });
        person.email = person
            .tours
            .iter()
            .find(|tour| !tour.guest_email.trim().is_empty())
            .map(|tour| tour.guest_email.clone())
            .unwrap_or_default()
    }
    people.sort_by(|a, b| b.tours[0].tour_date.cmp(&a.tours[0].tour_date));
    people
}
fn prospect_initials(name: &str) -> String {
    name.split_whitespace()
        .take(2)
        .filter_map(|part| part.chars().next())
        .collect::<String>()
        .to_uppercase()
}
fn prospect_contact(person: &Prospect) -> &str {
    if person.email.is_empty() || person.email.to_lowercase().ends_with("@example.invalid") {
        &person.phone
    } else {
        &person.email
    }
}

fn vacancy_prefill() -> Option<(i64, i64, String)> {
    let search = web_sys::window()?.location().search().ok()?;
    let parameter = |name: &str| {
        search.trim_start_matches('?').split('&').find_map(|pair| {
            pair.split_once('=')
                .and_then(|(key, value)| (key == name).then_some(value))
        })
    };
    let property = parameter("property")?
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)?;
    let room = parameter("room")?
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)?;
    let start = parameter("start")?.to_string();
    chrono::NaiveDate::parse_from_str(&start, "%Y-%m-%d").ok()?;
    Some((property, room, start))
}

pub fn lease_register_page() -> Element {
    let route = use_context::<Signal<String>>();
    let is_templates = route() == "/leases/templates";
    let mut leases = use_signal(Vec::<Lease>::new);
    let mut templates = use_signal(Vec::<LeaseTemplate>::new);
    let mut properties = use_signal(Vec::<Property>::new);
    let mut landlords = use_signal(Vec::<Landlord>::new);
    let mut lease_filter = use_signal(|| "all".to_string());
    use_effect(move || {
        spawn(async move {
            if let Ok(response) = Request::get("/api/leases").send().await {
                if let Ok(items) = response.json::<Vec<Lease>>().await {
                    leases.set(items);
                }
            }
            if let Ok(response) = Request::get("/api/lease-templates").send().await {
                if let Ok(items) = response.json::<Vec<LeaseTemplate>>().await {
                    templates.set(items);
                }
            }
            if let Ok(items) = api::list().await {
                properties.set(items);
            }
            if let Ok(response) = Request::get("/api/landlords").send().await {
                if let Ok(items) = response.json::<Vec<Landlord>>().await {
                    landlords.set(items);
                }
            }
        });
    });
    let all_leases = leases();
    let houses = properties();
    let owners = landlords();
    let visible_leases: Vec<_> = all_leases
        .iter()
        .filter(|lease| {
            lease_filter() == lease.status
                || (lease_filter() == "all" && lease.status != "confirmed")
        })
        .cloned()
        .collect();
    let all_templates = templates();
    rsx! {
        document::Title { "Leases | Buildry" }
        document::Stylesheet { href: crate::CSS }
        document::Script { src: "/assets/lease-section-swipe.mjs", r#type: "module" }
        crate::admin_shell::AdminShell { active: "/leases", property_count: None, main_class: "main-content lease-main lease-register-page",
            section { class: "page-head",
                h1 { "Leases" }
                div { class: "page-actions",
                    a { class: "header-search-action", href: if is_templates {"/leases/templates/search"} else {"/leases/search"}, aria_label: if is_templates {"Search templates"} else {"Search leases"}, title: if is_templates {"Search templates"} else {"Search leases"}, Icon { name: "search" } }
                    a { class: "primary-button page-create-action round-add-action", href: if is_templates {"/leases/templates/new"} else {"/leases/new"}, aria_label: if is_templates {"Create template"} else {"Create a lease"}, title: if is_templates {"Create template"} else {"Create a lease"}, Icon { name: "plus" } span { if is_templates {"Create template"} else {"Create a lease"} } }
                }
            }
            nav { class: "lease-section-nav", aria_label: "Lease sections",
                a { class: if is_templates {""} else {"active"}, href: "/leases", aria_current: if is_templates {"false"} else {"page"}, "Lease" }
                a { class: if is_templates {"active"} else {""}, href: "/leases/templates", aria_current: if is_templates {"page"} else {"false"}, "Templates" }
            }
            div { class: "lease-register-viewport",
                div { class: "lease-register-track",
                    section { class: "lease-register-pane", aria_label: "Leases",
                        crate::searchable_list::JointList { class: "lease-register", filters: Some(rsx! {
                            div { class: "searchable-list-filters lease-register-filters", role: "group", aria_label: "Filter leases",
                                for (key,label) in [("all","Active"),("sent","Sent"),("signed","Signed"),("confirmed","Confirmed")] {
                                    button { key: "{key}", r#type: "button", class: if lease_filter()==key {"active"} else {""}, onclick: move |_| lease_filter.set(key.to_string()), "{label}" }
                                }
                            }
                        }),
                            div { class: "searchable-list-items lease-register-list",
                                if all_leases.is_empty() { div { class: "lease-register-empty", span { Icon { name: "file" } } h3 { "No leases yet" } a { class: "primary-button round-add-action", href: "/leases/new", aria_label: "Create a lease", title: "Create a lease", Icon { name: "plus" } } } }
                                else if visible_leases.is_empty() { p { class: "lease-list-empty", "No leases match this view." } }
                                else { for lease in visible_leases {
                                    a { key: "{lease.id}", href: "/leases/view?lease={lease.id}", class: "lease-register-row register-row",
                                        span { class: "lease-prospect-avatar", "{prospect_initials(&lease.tenant_name)}" }
                                        span { class: "lease-register-copy", strong { "{lease.tenant_name}" } small { "{lease_property_name(&lease,&houses)}" } small { "#{lease.id} · C${lease.details.monthly_rent} / month" } }
                                        span { class: "lease-register-status", "{lease_status(&lease)}" }
                                        Icon { name: "arrow-right" }
                                    }
                                } }
                            }
                        }
                    }
                    section { class: "lease-register-pane", aria_label: "Lease templates",
                        crate::searchable_list::JointList { class: "template-list",
                            div { class: "searchable-list-items",
                                if all_templates.is_empty() { div { class: "lease-register-empty", h3 { "No templates yet" } } }
                                for template in all_templates {
                                    a { key: "{template.id}", class: "template-row register-row", href: "/leases/templates/new?id={template.id}",
                                        span { class: "lease-prospect-avatar", Icon { name: "file" } }
                                        span { class: "lease-register-copy", strong { "{template.name}" } small { "{template_landlord_name(&owners,template.landlord_id)}" } }
                                        span { class: "lease-register-status", "Template" }
                                        Icon { name: "arrow-right" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn leases_page() -> Element {
    let is_search_page = web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .is_some_and(|path| path == "/leases/search");
    let is_create_page = web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .is_some_and(|path| path == "/leases/new");
    let is_detail_page = web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .is_some_and(|path| path == "/leases/view");
    let vacancy_request = if is_create_page {
        vacancy_prefill()
    } else {
        None
    };
    let mut leases = use_signal(Vec::<Lease>::new);
    let mut templates = use_signal(Vec::<LeaseTemplate>::new);
    let mut templates_loaded = use_signal(|| false);
    let mut chosen_template = use_signal(|| 0_i64);
    let mut confirm_send = use_signal(|| false);
    let mut send_ready = use_signal(|| false);
    let mut slide_progress = use_signal(|| 0_u32);
    let mut properties = use_signal(Vec::<Property>::new);
    let mut tours = use_signal(Vec::<TourBooking>::new);
    let mut lease_landlords = use_signal(Vec::<Landlord>::new);
    let mut selected_tour = use_signal(|| {
        web_sys::window()
            .and_then(|w| w.location().search().ok())
            .and_then(|q| {
                q.strip_prefix("?tour=")
                    .and_then(|id| id.parse::<i64>().ok())
            })
            .unwrap_or(0)
    });
    let mut manual_guest = use_signal(|| false);
    let mut manual_first = use_signal(String::new);
    let mut manual_last = use_signal(String::new);
    let mut manual_email = use_signal(String::new);
    let mut selected_lease_id = use_signal(|| 0_i64);
    let mut creation_step = use_signal(|| 1_u8);
    let mut lease_filter = use_signal(|| "all".to_string());
    let mut lease_search = use_signal(String::new);
    let mut email_configured = use_signal(|| None::<bool>);
    let mut quick_rent = use_signal(String::new);
    let mut advanced = use_signal(|| false);
    let mut today = use_signal(String::new);
    let mut now_time = use_signal(String::new);
    let mut tours_loaded = use_signal(|| false);
    let mut choice_property = use_signal(|| {
        vacancy_request
            .as_ref()
            .map(|request| request.0)
            .unwrap_or(0)
    });
    let mut choice_property_query = use_signal(String::new);
    let mut wizard_search = use_signal(String::new);
    let mut wizard_search_open = use_signal(|| false);
    let mut choice_room = use_signal(|| {
        vacancy_request
            .as_ref()
            .map(|request| request.1)
            .unwrap_or(0)
    });
    let mut choice_start = use_signal(|| {
        vacancy_request
            .as_ref()
            .map(|request| request.2.clone())
            .unwrap_or_default()
    });
    let mut choice_months = use_signal(|| 12_i32);
    let mut custom_duration = use_signal(|| false);
    let mut custom_months = use_signal(String::new);
    let mut choice_email = use_signal(String::new);
    let mut edit_contact_email = use_signal(|| false);
    let mut tenant_name = use_signal(String::new);
    let mut tenant_email = use_signal(String::new);
    let mut property_id = use_signal(|| 0_i64);
    let mut room_id = use_signal(|| 0_i64);
    let mut editing_id = use_signal(|| 0_i64);
    let mut details = use_signal(Details::default);
    let mut message = use_signal(String::new);
    let mut busy = use_signal(|| false);
    use_effect(move || {
        spawn(async move {
            if let Ok(r) = Request::get("/api/leases").send().await {
                if let Ok(v) = r.json::<Vec<Lease>>().await {
                    let requested = web_sys::window()
                        .and_then(|w| w.location().search().ok())
                        .and_then(|q| {
                            q.strip_prefix("?lease=")
                                .and_then(|id| id.parse::<i64>().ok())
                        });
                    selected_lease_id.set(
                        requested
                            .filter(|id| v.iter().any(|lease| lease.id == *id))
                            .or_else(|| {
                                v.iter()
                                    .find(|lease| lease.status != "draft")
                                    .map(|lease| lease.id)
                            })
                            .unwrap_or(0),
                    );
                    leases.set(v)
                }
            }
            if let Ok(r) = Request::get("/api/lease-templates").send().await {
                if let Ok(v) = r.json().await {
                    templates.set(v)
                }
            }
            templates_loaded.set(true);
            if let Ok(v) = api::list().await {
                properties.set(v)
            }
            if let Ok(r) = Request::get("/api/landlords").send().await {
                if let Ok(v) = r.json().await {
                    lease_landlords.set(v)
                }
            }
            if let Ok(r) = Request::get("/api/email/status").send().await {
                if let Ok(v) = r.json::<EmailStatus>().await {
                    email_configured.set(Some(v.configured))
                }
            }
            if let Ok(r) = Request::get("/api/tours/admin-options").send().await {
                if let Ok(v) = r.json::<crate::tour_model::TourOptions>().await {
                    today.set(v.today);
                    now_time.set(v.now_time)
                }
            }
            if let Ok(r) = Request::get("/api/tours").send().await {
                if let Ok(v) = r.json::<Vec<TourBooking>>().await {
                    if let Some(tour) = v.iter().find(|tour| tour.id == selected_tour()) {
                        creation_step.set(1);
                        choice_property.set(tour.property_id as i64);
                        choice_room.set(tour.room_id.unwrap_or(0) as i64);
                        chosen_template.set(0);
                        choice_start.set(tour.rental_start.clone().unwrap_or_default());
                        let months = tour.rental_months.unwrap_or(12) as i32;
                        choice_months.set(months);
                        custom_duration.set(![4, 8, 12, 24].contains(&months));
                        custom_months.set(if [4, 8, 12, 24].contains(&months) {
                            String::new()
                        } else {
                            months.to_string()
                        });
                        let email = prospects_from_tours(&v)
                            .into_iter()
                            .find(|person| person.tours.iter().any(|item| item.id == tour.id))
                            .map(|person| person.email)
                            .unwrap_or_default();
                        choice_email.set(email.clone());
                        edit_contact_email
                            .set(email.is_empty() || email.ends_with("@example.invalid"));
                        quick_rent.set(
                            properties()
                                .iter()
                                .find(|house| house.id == tour.property_id)
                                .and_then(|house| {
                                    house
                                        .spaces
                                        .iter()
                                        .find(|room| Some(room.id) == tour.room_id)
                                })
                                .and_then(crate::pricing_page::current_room_price)
                                .map(|rent| rent.to_string())
                                .unwrap_or_default(),
                        );
                    }
                    tours.set(v)
                }
            }
            tours_loaded.set(true);
        });
    });
    use_effect(move || {
        if !send_ready() {
            return;
        }
        send_ready.set(false);
        if busy() {
            return;
        }
        busy.set(true);
        let months = if custom_duration() {
            custom_months().parse::<i32>().unwrap_or(0)
        } else {
            choice_months()
        };
        let whole_house = choice_room() == -1;
        let start = choice_start();
        let end = chrono::NaiveDate::parse_from_str(&start, "%Y-%m-%d")
            .ok()
            .and_then(|date| date.checked_add_months(chrono::Months::new(months.max(0) as u32)))
            .and_then(|date| date.pred_opt())
            .map(|date| date.to_string())
            .unwrap_or_default();
        let mut lease_details = Details::default();
        lease_details.start_date = start.clone();
        lease_details.end_date = end;
        lease_details.term_type = "fixed".into();
        lease_details.monthly_rent = quick_rent();
        lease_details.rent_due_day = "1".into();
        lease_details.rent_payment_method = "e-transfer".into();
        lease_details.rent_deposit = quick_rent();
        lease_details.unit_description = if whole_house {
            "Entire house".into()
        } else {
            properties()
                .iter()
                .find(|house| house.id as i64 == choice_property())
                .and_then(|house| {
                    house
                        .spaces
                        .iter()
                        .find(|room| room.id as i64 == choice_room())
                })
                .map(|room| room.name.clone())
                .unwrap_or_else(|| "Room".into())
        };
        let request = if manual_guest() {
            let body = serde_json::json!({"tenant_name":format!("{} {}",manual_first().trim(),manual_last().trim()),"tenant_email":choice_email(),"property_id":choice_property(),"room_id":if whole_house{0}else{choice_room()},"lease_type":if whole_house{"entire_house"}else{"room"},"details":lease_details,"template_id":chosen_template()});
            Request::post("/api/leases").json(&body)
        } else {
            let body = serde_json::json!({"monthly_rent":quick_rent().parse::<u32>().unwrap_or(0),"property_id":choice_property(),"room_id":if whole_house{0}else{choice_room()},"lease_type":if whole_house{"entire_house"}else{"room"},"start_date":start,"months":months,"tenant_email":choice_email(),"template_id":chosen_template()});
            Request::post(&format!("/api/prospects/{}/lease", selected_tour())).json(&body)
        };
        spawn(async move {
            match request {
                Ok(request) => match request.send().await {
                    Ok(response) if response.ok() => {
                        if let Ok(created) = response.json::<Lease>().await {
                            let _ = web_sys::window().map(|window| {
                                window
                                    .location()
                                    .set_href(&format!("/leases/view?lease={}", created.id))
                            });
                        } else {
                            message.set("Lease sent, but its details could not be opened. Check the lease register.".into());
                            confirm_send.set(false);
                        }
                    }
                    Ok(response) => {
                        message.set(error(response).await);
                        confirm_send.set(false)
                    }
                    Err(_) => {
                        message.set("Could not create and email the lease.".into());
                        confirm_send.set(false)
                    }
                },
                Err(_) => {
                    message.set("Could not prepare the lease request.".into());
                    confirm_send.set(false)
                }
            }
            busy.set(false);
            slide_progress.set(0);
        });
    });
    let d = details();
    let houses = properties();
    let rooms: Vec<_> = houses
        .iter()
        .find(|h| h.id as i64 == property_id())
        .map(|h| {
            h.spaces
                .iter()
                .filter(|s| s.kind == "Bedroom" || s.kind == "Suite")
                .collect()
        })
        .unwrap_or_default();
    let prospects = prospects_from_tours(&tours());
    let selected_prospect = prospects
        .iter()
        .find(|person| person.tours.iter().any(|tour| tour.id == selected_tour()))
        .cloned();
    let selected_guest_name = if manual_guest() {
        format!("{} {}", manual_first().trim(), manual_last().trim())
            .trim()
            .to_string()
    } else {
        selected_prospect
            .as_ref()
            .map(|person| person.name.clone())
            .unwrap_or_else(|| "Guest".into())
    };
    let all_leases = leases();
    let selected_management_lease = all_leases
        .iter()
        .find(|lease| lease.id == selected_lease_id())
        .cloned();
    let visible_leases: Vec<_> = all_leases
        .iter()
        .filter(|lease| {
            let matches_status = lease_filter() == lease.status
                || (lease_filter() == "all" && (is_search_page || lease.status != "confirmed"));
            let query = lease_search().to_lowercase();
            let property_name = houses
                .iter()
                .find(|house| house.id as i64 == lease.property_id)
                .map(|house| house.name.as_str())
                .unwrap_or("");
            matches_status
                && (query.is_empty()
                    || format!(
                        "{} {} {} {}",
                        lease.tenant_name, lease.tenant_email, property_name, lease.id
                    )
                    .to_lowercase()
                    .contains(&query))
        })
        .cloned()
        .collect();
    let management_property = selected_management_lease.as_ref().and_then(|lease| {
        houses
            .iter()
            .find(|house| house.id as i64 == lease.property_id)
    });
    let management_room = selected_management_lease
        .as_ref()
        .map(|lease| {
            if lease.lease_type == "entire_house" {
                "Entire house".into()
            } else {
                management_property
                    .and_then(|house| {
                        house
                            .spaces
                            .iter()
                            .find(|space| space.id as i64 == lease.room_id)
                    })
                    .map(|space| space.name.clone())
                    .unwrap_or_else(|| "Room".into())
            }
        })
        .unwrap_or_else(|| "Room".into());
    let management_property_name = management_property
        .map(|house| house.name.clone())
        .unwrap_or_else(|| "Property".into());
    let management_landlord_name = selected_management_lease
        .as_ref()
        .and_then(|lease| {
            lease_landlords()
                .iter()
                .find(|landlord| landlord.property_ids.contains(&lease.property_id))
                .map(|landlord| landlord.legal_name.clone())
        })
        .unwrap_or_else(|| "Not assigned".into());
    let management_landlord_id = selected_management_lease.as_ref().and_then(|lease| {
        lease_landlords()
            .iter()
            .find(|landlord| landlord.property_ids.contains(&lease.property_id))
            .map(|landlord| landlord.id)
    });
    let selected_property = houses
        .iter()
        .find(|house| house.id as i64 == choice_property());
    let property_query = choice_property_query().to_lowercase();
    let selected_template_landlord = templates()
        .iter()
        .find(|template| template.id == chosen_template())
        .and_then(|template| template.landlord_id);
    let visible_choice_properties: Vec<_> = houses
        .iter()
        .filter(|house| {
            selected_template_landlord.is_some_and(|landlord_id| {
                lease_landlords().iter().any(|landlord| {
                    landlord.id == landlord_id && landlord.property_ids.contains(&(house.id as i64))
                })
            }) && (property_query.is_empty()
                || format!("{} {}", house.name, house.address)
                    .to_lowercase()
                    .contains(&property_query))
        })
        .collect();
    let selected_landlord_name = lease_landlords()
        .into_iter()
        .find(|landlord| landlord.property_ids.contains(&choice_property()))
        .map(|landlord| landlord.legal_name)
        .unwrap_or_else(|| "Not assigned".into());
    let selected_template_valid = templates().iter().any(|template| {
        template.id == chosen_template()
            && lease_landlords().iter().any(|landlord| {
                Some(landlord.id) == template.landlord_id
                    && landlord.property_ids.contains(&choice_property())
            })
    });
    let selected_rooms: Vec<_> = selected_property
        .map(|house| {
            house
                .spaces
                .iter()
                .filter(|space| space.kind == "Bedroom" || space.kind == "Suite")
                .collect()
        })
        .unwrap_or_default();
    let selected_property_name = selected_property
        .map(|house| house.name.clone())
        .unwrap_or_else(|| "Property".into());
    let selected_property_image = selected_property
        .map(|house| {
            house
                .photos
                .first()
                .cloned()
                .unwrap_or_else(|| crate::guest_pages::house_photo(house.id))
        })
        .unwrap_or_default();
    let selected_guest_phone = selected_prospect
        .as_ref()
        .map(|person| person.phone.clone())
        .unwrap_or_default();
    let selected_room_name = if choice_room() == -1 {
        "Entire house".to_string()
    } else {
        selected_rooms
            .iter()
            .find(|room| room.id as i64 == choice_room())
            .map(|room| room.name.clone())
            .unwrap_or_else(|| "Room".into())
    };
    let selected_template_name = templates()
        .iter()
        .find(|template| template.id == chosen_template())
        .map(|template| template.name.clone())
        .unwrap_or_else(|| "Choose a template".into());
    let wizard_query = wizard_search().to_lowercase();
    let mut visible_prospects: Vec<_> = prospects
        .iter()
        .filter(|person| {
            !wizard_search_open()
                || wizard_query.is_empty()
                || format!("{} {} {}", person.name, person.email, person.phone)
                    .to_lowercase()
                    .contains(&wizard_query)
        })
        .cloned()
        .collect();
    visible_prospects.sort_by(|a, b| {
        let a_future = a
            .tours
            .iter()
            .any(|tour| tour_upcoming(tour, &today(), &now_time()));
        let b_future = b
            .tours
            .iter()
            .any(|tour| tour_upcoming(tour, &today(), &now_time()));
        b_future
            .cmp(&a_future)
            .then_with(|| b.tours[0].tour_date.cmp(&a.tours[0].tour_date))
    });
    let chosen_months = if custom_duration() {
        custom_months().parse::<u32>().ok()
    } else {
        u32::try_from(choice_months()).ok()
    };
    let chosen_stay = chrono::NaiveDate::parse_from_str(&choice_start(), "%Y-%m-%d")
        .ok()
        .and_then(|start| {
            chosen_months
                .filter(|months| (1..=120).contains(months))
                .and_then(|months| start.checked_add_months(chrono::Months::new(months)))
                .and_then(|next| next.pred_opt())
                .map(|end| (start, end))
        });
    let selected_unit_id = if choice_room() == -1 {
        0
    } else {
        choice_room()
    };
    let stay_conflict = chosen_stay.is_some_and(|(start, end)| {
        all_leases.iter().any(|lease| {
            lease_overlaps_stay(lease, choice_property(), selected_unit_id, start, end)
        })
    });
    let guest_ready = if manual_guest() {
        !manual_first().trim().is_empty()
            && !manual_last().trim().is_empty()
            && manual_email().contains('@')
    } else {
        selected_tour() != 0
    };
    let can_create_lease = !busy()
        && email_configured() == Some(true)
        && guest_ready
        && choice_property() != 0
        && choice_room() != 0
        && chosen_stay.is_some()
        && !stay_conflict
        && choice_email().contains('@')
        && !choice_email().to_lowercase().ends_with("@example.invalid")
        && selected_template_valid
        && quick_rent().parse::<u32>().unwrap_or(0) > 0;
    let slide_left = format!(
        "calc({}% - {}px + 8px)",
        slide_progress(),
        slide_progress() * 58 / 100
    );
    if is_search_page {
        return rsx! {
            document::Title { "Search leases | Buildry" }
            document::Stylesheet { href: crate::CSS }
            crate::admin_shell::AdminShell { active: "/leases", property_count: None, main_class: "main-content lease-main search-page-main",
                crate::searchable_list::SearchScreen { back_href: "/leases", placeholder: "Search leases", value: lease_search(), count: visible_leases.len(), on_search: move |value| lease_search.set(value),
                    crate::searchable_list::JointList { class: "lease-register", filters: Some(rsx! {
                        div { class: "searchable-list-filters lease-register-filters", role: "group", aria_label: "Filter leases",
                            for (key,label) in [("all","All"),("sent","Sent"),("signed","Signed"),("confirmed","Confirmed")] {
                                button { key: "{key}", r#type: "button", class: if lease_filter()==key {"active"} else {""}, onclick: move |_| lease_filter.set(key.to_string()), "{label}" }
                            }
                        }
                    }),
                        div { class: "searchable-list-items lease-register-list",
                            if all_leases.is_empty() { p { class: "lease-list-empty", "No leases yet." } }
                            else if visible_leases.is_empty() { p { class: "lease-list-empty", "No leases match that search." } }
                            else { for lease in visible_leases {
                                a { key: "{lease.id}", href: "/leases/view?lease={lease.id}", class: "lease-register-row register-row",
                                    span { class: "lease-prospect-avatar", "{prospect_initials(&lease.tenant_name)}" }
                                    span { class: "lease-register-copy", strong { "{lease.tenant_name}" } small { "{lease_property_name(&lease,&houses)}" } small { "#{lease.id} · C${lease.details.monthly_rent} / month" } }
                                    span { class: "lease-register-status", "{lease_status(&lease)}" }
                                    Icon { name: "arrow-right" }
                                }
                            } }
                        }
                    }
                }
            }
        };
    }
    rsx! { Shell { title: if is_create_page {"Create a lease"}else if is_detail_page {"Lease details"}else{"Leases"}, active: "/leases",
        if !is_create_page && !is_detail_page {nav {class:"lease-section-nav",aria_label:"Lease sections",
            a {class:"active",href:"/leases","Lease"}
            a {href:"/leases/templates","Templates"}
        }}
        if !is_search_page {
        if !is_create_page && !is_detail_page {
        if !message().is_empty() { p { class: "lease-notice", role: "status", "{message}" } }
        if email_configured()==Some(false) { p { class: "lease-notice", role: "status", "Email sending is not configured. Add the Resend sender and API key to the service before emailing leases." } }
        div {class:"lease-management lease-section-content",
            crate::searchable_list::JointList {class:"lease-register",filters:Some(rsx! {
                div {class:"searchable-list-filters lease-register-filters",role:"group",aria_label:"Filter leases",for (key,label) in [("all","Active"),("sent","Sent"),("signed","Signed"),("confirmed","Confirmed")] {button {key:"{key}",r#type:"button",class:if lease_filter()==key{"active"}else{""},onclick:move |_|lease_filter.set(key.to_string()),"{label}"}}}
            }),
                div {class:"searchable-list-items lease-register-list",if all_leases.is_empty(){div {class:"lease-register-empty",span {Icon { name: "file" }} h3 {"No leases yet"} a {class:"primary-button round-add-action",href:"/leases/new",aria_label:"Create a lease",title:"Create a lease",Icon { name: "plus" }}}} else if visible_leases.is_empty(){p {class:"lease-list-empty","No leases match this view."}} else {for lease in visible_leases {a {key:"{lease.id}",href:"/leases/view?lease={lease.id}",class:"lease-register-row register-row",span {class:"lease-prospect-avatar","{prospect_initials(&lease.tenant_name)}"} span {class:"lease-register-copy",strong {"{lease.tenant_name}"} small {"{lease_property_name(&lease,&houses)}"} small {"#{lease.id} · C${lease.details.monthly_rent} / month"}} span {class:"lease-register-status","{lease_status(&lease)}"} Icon { name: "arrow-right" }}}}}
            }
        }
        }
        if is_detail_page {div {class:"lease-detail-page",
            crate::form_action_bar::FormActionBar {back_href:"/leases".to_string(),back_label:"Back to leases".to_string(),title:Some("Lease details".to_string()),
                if let Some(lease)=selected_management_lease.clone() {if lease.status=="confirmed" {button {class:"lease-header-icon",r#type:"button",aria_label:"Resend signed copy",title:"Resend signed copy",disabled:busy(),onclick:{let id=lease.id;move |_|{busy.set(true);spawn(async move {match Request::post(&format!("/api/leases/{id}/resend-confirmation")).send().await {Ok(r) if r.ok()=>message.set("Signed lease copy sent.".into()),Ok(r)=>message.set(error(r).await),Err(_)=>message.set("Could not resend the signed copy.".into())}busy.set(false);});}},Icon {name:"mail"}}}}
            }
            div {class:"lease-detail-heading lease-wizard-heading",
                span {"LEASE DETAILS"}
                h1 {"Lease details"}
            }
            if !message().is_empty() { p {class:"lease-notice",role:"status","{message}"} }
            section {class:"lease-detail-sheet",aria_label:"Lease details",
                if let Some(lease)=selected_management_lease.clone(){
                    div {class:"lease-management-top",span {"LEASE #{lease.id}"} span {class:"lease-register-status","{lease_status(&lease)}"}}
                    h2 {"{lease.tenant_name}"} p {class:"lease-management-email","{lease.tenant_email}"}
                    if lease.status=="draft" {p {class:"lease-delivery-note","Confirm the tenant's legal name above before emailing. Use Edit terms to correct it."}}
                    if lease.is_demo {p {class:"lease-delivery-note","Demonstration only · This test lease cannot be confirmed as a tenancy."}}
                    AdminLeaseDetails { lease: lease.clone(), property_name: management_property_name.clone(), property_address: management_property.map(|house|house.address.clone()).unwrap_or_default(), room_name: management_room.clone(), landlord_name: management_landlord_name.clone() }
                    if lease.status=="draft" {div {class:"lease-template-picker",
                        FormSelect {label:"Template for this lease",required:true,value:chosen_template().to_string(),placeholder:"Choose a template",options:templates().iter().filter(|template|template.landlord_id==management_landlord_id).map(|template|(template.id.to_string(),template.name.clone())).collect::<Vec<_>>(),onchange:move |value:String|chosen_template.set(value.parse().unwrap_or(0))}
                        button {class:"outline-button",r#type:"button",disabled:busy()||chosen_template()==0,onclick:{let id=lease.id;move |_|{busy.set(true);spawn(async move {let body=serde_json::json!({"template_id":chosen_template()});match Request::post(&format!("/api/leases/{id}/template")).json(&body).unwrap().send().await {Ok(r) if r.ok()=>{if let Ok(r)=Request::get("/api/leases").send().await {if let Ok(v)=r.json().await {leases.set(v)}}message.set("Template applied. Review the PDF before sending.".into())},Ok(r)=>message.set(error(r).await),Err(_)=>message.set("Could not apply template.".into())}busy.set(false);});}},"Use template"}
                        if let Some(template_id)=lease.template_id {span {"Using template #{template_id}"}}
                    }}
                    if lease.tenant_email.to_lowercase().ends_with("@example.invalid") {p {class:"lease-delivery-note","Confirm the guest email in Edit terms before sending."}}
                    div {class:"lease-management-actions",
                        if lease.status=="draft" {button {class:"outline-button",r#type:"button",onclick:{let lease=lease.clone();move |_|{editing_id.set(lease.id);tenant_name.set(lease.tenant_name.clone());tenant_email.set(lease.tenant_email.clone());property_id.set(lease.property_id);room_id.set(if lease.lease_type=="entire_house"{-1}else{lease.room_id});details.set(lease.details.clone());advanced.set(true)}},"Edit terms"}}
                        if lease.status=="draft"||lease.status=="sent" {button {class:"primary-button",r#type:"button",disabled:busy()||email_configured()==Some(false)||lease.tenant_email.to_lowercase().ends_with("@example.invalid"),onclick:{let id=lease.id;move |_|{busy.set(true);spawn(async move {match Request::post(&format!("/api/leases/{id}/send")).send().await {Ok(r) if r.ok()=>{if let Ok(r)=Request::get("/api/leases").send().await {if let Ok(v)=r.json().await {leases.set(v)}}message.set("Lease email sent.".into())},Ok(r)=>message.set(error(r).await),Err(_)=>message.set("Could not send lease.".into())}busy.set(false);});}},if lease.status=="sent"{"Resend tenant signing link"}else{"Email tenant to sign"}}}
                    }
                    if lease.status=="signed" && lease.landlord_signed_at.is_none() && !lease.is_demo {
                        section {class:"lease-id-review",aria_label:"Landlord signature",
                            div {class:"lease-id-review-heading",div {span {class:"lease-step-label","NEXT STEP"} h3 {"Landlord signature"} p {"The tenant has signed. A private signing link was emailed to the landlord."}}
                                span {class:"lease-id-status","Awaiting signature"}
                            }
                            button {class:"outline-button",r#type:"button",disabled:busy(),onclick:{let id=lease.id;move |_|{busy.set(true);spawn(async move {match Request::post(&format!("/api/leases/{id}/landlord-link")).send().await {Ok(r) if r.ok()=>message.set("Landlord signing link sent.".into()),Ok(r)=>message.set(error(r).await),Err(_)=>message.set("Could not email the landlord signing link.".into())}busy.set(false);});}},"Resend landlord link"}
                        }
                    }
                    if lease.status=="signed" && lease.landlord_signed_at.is_some() && !lease.is_demo {
                        section {class:"lease-id-review",aria_label:"Identity verification",
                            div {class:"lease-id-review-heading",div {span {class:"lease-step-label","NEXT STEP"} h3 {"Verify identity"} p {if lease.photo_id_verified {"The tenant's ID is approved. You can confirm the tenancy."} else if lease.photo_id_uploaded {"Review the uploaded ID, then approve it or request a new upload."} else {"Waiting for the tenant to upload a photo ID."}}}
                                span {class:if lease.photo_id_verified {"lease-id-status complete"} else if lease.photo_id_uploaded {"lease-id-status ready"} else {"lease-id-status"},if lease.photo_id_verified {"Approved"} else if lease.photo_id_uploaded {"Ready to review"} else {"Waiting"}}
                            }
                            if lease.photo_id_uploaded && !lease.photo_id_verified {
                                div {class:"lease-id-documents",
                                    a {href:"/api/leases/{lease.id}/photo-id/front",target:"_blank",rel:"noopener noreferrer","View front" Icon {name:"arrow-up-right"}}
                                    if lease.photo_id_type.as_deref()!=Some("passport") {a {href:"/api/leases/{lease.id}/photo-id/back",target:"_blank",rel:"noopener noreferrer","View back" Icon {name:"arrow-up-right"}}}
                                }
                                div {class:"lease-id-review-actions",
                                    button {class:"primary-button",r#type:"button",disabled:busy(),onclick:{let id=lease.id;move |_|{busy.set(true);spawn(async move {match Request::post(&format!("/api/leases/{id}/photo-id/approve")).send().await {Ok(r) if r.ok()=>{if let Ok(r)=Request::get("/api/leases").send().await {if let Ok(v)=r.json().await {leases.set(v)}}message.set("Photo ID approved.".into())},Ok(r)=>message.set(error(r).await),Err(_)=>message.set("Could not approve photo ID.".into())}busy.set(false);});}},"Approve ID"}
                                    button {class:"text-button",r#type:"button",disabled:busy(),onclick:{let id=lease.id;move |_|{busy.set(true);spawn(async move {match Request::post(&format!("/api/leases/{id}/photo-id/reject")).send().await {Ok(r) if r.ok()=>{if let Ok(r)=Request::get("/api/leases").send().await {if let Ok(v)=r.json().await {leases.set(v)}}message.set("ID removed. Email a new upload link to the tenant.".into())},Ok(r)=>message.set(error(r).await),Err(_)=>message.set("Could not reject photo ID.".into())}busy.set(false);});}},"Request new ID"}
                                }
                            }
                            if !lease.photo_id_uploaded {button {class:"outline-button",r#type:"button",disabled:busy(),onclick:{let id=lease.id;move |_|{busy.set(true);spawn(async move {match Request::post(&format!("/api/leases/{id}/photo-id-link")).send().await {Ok(r) if r.ok()=>message.set("A new private ID upload link was emailed.".into()),Ok(r)=>message.set(error(r).await),Err(_)=>message.set("Could not email an ID upload link.".into())}busy.set(false);});}},"Resend upload link"}}
                            if lease.photo_id_verified {button {class:"primary-button",r#type:"button",disabled:busy(),onclick:{let id=lease.id;move |_|{busy.set(true);spawn(async move {match Request::post(&format!("/api/leases/{id}/confirm")).send().await {Ok(r) if r.ok()=>{if let Ok(r)=Request::get("/api/leases").send().await {if let Ok(v)=r.json().await {leases.set(v)}}message.set("Lease confirmed.".into())},Ok(r)=>message.set(error(r).await),Err(_)=>message.set("Could not confirm lease.".into())}busy.set(false);});}},"Confirm tenancy"}}
                        }
                    }
                    if lease.copy_sent_at.is_none()&&lease.status=="confirmed" {p {class:"lease-delivery-note","Signed copy delivery needs attention. Use the mail icon in the header to resend."}}
                    section {class:"signing-document lease-document-preview",
                        div {class:"signing-section-head",div {h2 {"Lease PDF"} p {"Ontario Standard Form of Lease"}} a {class:"lease-pdf-open-button",href:"/api/leases/{lease.id}/document",target:"_blank",rel:"noopener noreferrer",aria_label:"Open PDF",title:"Open PDF",Icon {name:"arrow-up-right"}}}
                        div {class:"lease-pdf-viewer lease-pdf-scroll-viewer","data-document":"/api/leases/{lease.id}/document","data-signed":if lease.status=="signed"||lease.status=="confirmed"{"true"}else{"false"},
                        }
                    }
                } else {div {class:"lease-select-empty",span {class:"lease-empty-symbol",Icon { name: "file" }} h2 {"Select a lease"}}}
            }
        }}
        if is_create_page {
            div { class: if wizard_search_open() {"lease-create-page wizard-search-open"}else{"lease-create-page"},
                crate::form_action_bar::FormActionBar {
                        back_href: String::new(),
                        back_label: "Close search".to_string(),
                        class_name: Some(if wizard_search_open() {"lease-wizard-search-bar"} else {"lease-wizard-search-bar inactive"}.to_string()),
                        on_back: move |_| wizard_search_open.set(false),
                        middle: Some(rsx! { crate::form_input::FormInput { r#type: "search", aria_label: match creation_step() {1=>"Search templates",2=>"Search guests",_=>"Search properties"}, placeholder: match creation_step() {1=>"Search templates",2=>"Search guests",_=>"Search properties"}, autofocus:wizard_search_open(), value: if creation_step()==3 {choice_property_query()} else {wizard_search()}, oninput: move |event: FormEvent| {if creation_step()==3 {choice_property_query.set(event.value())} else {wizard_search.set(event.value())}} } }),
                    }
                crate::form_action_bar::FormActionBar {
                    back_href: "/leases".to_string(),
                    back_label: "Back to leases".to_string(),
                    class_name: Some(if wizard_search_open() {"lease-wizard-main-bar inactive"} else {"lease-wizard-main-bar"}.to_string()),
                    back_step: if creation_step() > 1 { Some(creation_step) } else { None },
                    title:Some(match creation_step(){1=>"Choose a template",2=>"Choose a guest",3=>"Choose a property",4=>"Choose a rental unit",_=>"Set the stay and rent"}.to_string()),
                    if creation_step() <= 3 { button { class: "form-action-icon", r#type: "button", aria_label: match creation_step(){1=>"Search templates",2=>"Search guests",_=>"Search properties"}, title: match creation_step(){1=>"Search templates",2=>"Search guests",_=>"Search properties"}, onclick: move |_| {wizard_search.set(String::new());choice_property_query.set(String::new());wizard_search_open.set(true);focus_lease_search()}, Icon { name: "search" } } }
                    if creation_step() == 5 { button { class: "property-save-button", r#type: "button", aria_label: "Email lease", title: "Email lease", disabled: !can_create_lease, onclick: move |_| confirm_send.set(true), Icon { name: "check" } } }
                }
                if !message().is_empty() { p { class: "lease-notice", role: "status", "{message}" } }
                if !wizard_search_open() { crate::form_action_bar::FormWizardHeader {
                    eyebrow: "CREATE A LEASE".to_string(),
                    title: match creation_step() { 1 => "Choose a template", 2 => "Choose a guest", 3 => "Choose a property", 4 => "Choose a rental unit", _ => "Set the stay and rent" }.to_string(),
                    description: String::new(),
                    step: creation_step(),
                    total: 5,
                } }
                section { class: "lease-wizard-card",
                    if creation_step() == 1 {
                        div { class: "lease-wizard-body",
                            if !templates_loaded() { p { class: "lease-list-empty", "Loading templates…" } }
                            else if templates().is_empty() {
                                div { class: "lease-wizard-empty", p { "No templates are available yet." } a { class: "outline-button", href: "/leases/templates/new", "Create a template" } }
                            }
                            else if wizard_search_open() && !templates().iter().any(|template|format!("{} {}",template.name,template_landlord_name(&lease_landlords(),template.landlord_id)).to_lowercase().contains(&wizard_query)) { p {class:"lease-list-empty","No templates match that search."} }
                            crate::searchable_list::JointList { class: "lease-wizard-joint-list",
                                for template in templates().iter().filter(|template| !wizard_search_open() || wizard_query.is_empty() || format!("{} {}",template.name,template_landlord_name(&lease_landlords(),template.landlord_id)).to_lowercase().contains(&wizard_query)) {
                                    button { key: "{template.id}", r#type: "button", class: "lease-wizard-option", onclick: { let id = template.id; move |_| { if chosen_template() != id && selected_tour()==0 { choice_property.set(0); choice_room.set(0); quick_rent.set(String::new()); } chosen_template.set(id); wizard_search_open.set(false); wizard_search.set(String::new()); show_lease_step(creation_step,2); } },
                                        span { class: "lease-wizard-option-icon", Icon { name: "file" } }
                                        span { strong { "{template.name}" } small { "{template_landlord_name(&lease_landlords(), template.landlord_id)}" } }
                                    }
                                }
                            }
                        }
                    }
                    if creation_step() == 2 {
                        div { class: "lease-wizard-body lease-guest-step",
                            crate::collapsible_panel::CollapsibleCard { class: "lease-manual-guest", summary: rsx! { Icon { name: "plus" } "Add a new guest" },
                                crate::app_form::AppForm { class: "lease-manual-fields", onsubmit: move |event: FormEvent| {event.prevent_default();if manual_first().trim().is_empty() || manual_last().trim().is_empty() || !manual_email().contains('@'){return}manual_guest.set(true);selected_tour.set(0);choice_email.set(manual_email());edit_contact_email.set(false);wizard_search_open.set(false);show_lease_step(creation_step,3);},
                                    crate::app_form::FormRows { class: Some("lease-manual-rows".to_string()),
                                        FormField { label: "First name", required: true, crate::form_input::FormInput { required: true, aria_label: "First name", value: "{manual_first}", autocomplete: "given-name", oninput: move |e: FormEvent| manual_first.set(e.value()) } }
                                        FormField { label: "Last name", required: true, crate::form_input::FormInput { required: true, aria_label: "Last name", value: "{manual_last}", autocomplete: "family-name", oninput: move |e: FormEvent| manual_last.set(e.value()) } }
                                        FormField { label: "Email address", required: true, crate::form_input::FormInput { r#type: "email", required: true, aria_label: "Email address", value: "{manual_email}", autocomplete: "email", oninput: move |e: FormEvent| {let value=e.value();if manual_guest(){choice_email.set(value.clone())}manual_email.set(value)} } }
                                    }
                                    button { class: "outline-button", r#type: "submit", disabled: manual_first().trim().is_empty() || manual_last().trim().is_empty() || !manual_email().contains('@'), "Use this guest" }
                                }
                            }
                            div { class: "lease-guest-results-card",
                                if !tours_loaded() { p { class: "lease-list-empty", "Loading potential guests…" } }
                                else if visible_prospects.is_empty() { p { class: "lease-list-empty", if wizard_search_open(){"No guests match that search."}else{"No guests available yet."} } }
                                crate::searchable_list::JointList { class: "lease-wizard-joint-list",
                                    for person in visible_prospects {
                                    button { key: "{person.key}", r#type: "button", class: "lease-wizard-option", onclick: { let person = person.clone(); let vacancy_request = vacancy_request.clone(); move |_| {
                                        let tour = &person.tours[0];
                                        selected_tour.set(tour.id); manual_guest.set(false);
                                        choice_property.set(vacancy_request.as_ref().map(|request| request.0).unwrap_or(tour.property_id as i64));
                                        choice_room.set(vacancy_request.as_ref().map(|request| request.1).unwrap_or(tour.room_id.unwrap_or(0) as i64));
                                        choice_start.set(vacancy_request.as_ref().map(|request| request.2.clone()).unwrap_or_else(|| tour.rental_start.clone().unwrap_or_default()));
                                        let months = tour.rental_months.unwrap_or(12) as i32;
                                        choice_months.set(months); custom_duration.set(![4, 8, 12, 24].contains(&months));
                                        custom_months.set(if [4, 8, 12, 24].contains(&months) { String::new() } else { months.to_string() });
                                        choice_email.set(person.email.clone()); edit_contact_email.set(person.email.is_empty() || person.email.ends_with("@example.invalid"));
                                        quick_rent.set(properties().iter().find(|house| house.id == tour.property_id).and_then(|house| house.spaces.iter().find(|room| Some(room.id) == tour.room_id)).and_then(crate::pricing_page::current_room_price).map(|rent| rent.to_string()).unwrap_or_default());
                                        wizard_search_open.set(false); wizard_search.set(String::new()); show_lease_step(creation_step,3);
                                    } },
                                        span { class: "lease-prospect-avatar", "{prospect_initials(&person.name)}" }
                                        span { strong { "{person.name}" } small { "{prospect_contact(&person)}" } }
                                    }
                                }
                                }
                            }
                        }
                    }
                    if creation_step() == 3 {
                        div { class: "lease-wizard-body",
                            if visible_choice_properties.is_empty() { p { class: "lease-list-empty", "No properties for this template match your search." } }
                            crate::searchable_list::JointList { class: "lease-wizard-joint-list",
                                for house in visible_choice_properties {
                                    button { key:"{house.id}", r#type:"button", class:"lease-wizard-option", onclick:{let id=house.id as i64;move |_|{if choice_property()!=id{choice_room.set(0);quick_rent.set(String::new())}choice_property.set(id);choice_property_query.set(String::new());wizard_search_open.set(false);show_lease_step(creation_step,4)}},
                                        img {src:house.photos.first().map(|photo|crate::searchable_list::cover_thumbnail_url(photo)).unwrap_or_else(||crate::guest_pages::house_photo(house.id)),alt:"",loading:"lazy",decoding:"async"}
                                        span {strong {"{house.name}"} small {"{house.location_label()}"}}
                                    }
                                }
                            }
                        }
                    }
                    if creation_step() == 4 {
                        div { class: "lease-wizard-body",
                            crate::searchable_list::JointList { class: "lease-wizard-joint-list",
                                button { r#type: "button", class: "lease-wizard-option", onclick: move |_| { choice_room.set(-1); quick_rent.set(String::new()); show_lease_step(creation_step,5); },
                                        span { class: "lease-wizard-option-icon", Icon { name: "house" } }
                                    span { strong { "Entire house" } small { "Enter the agreed monthly rent" } }
                                }
                                for room in &selected_rooms {
                                    button { key: "{room.id}", r#type: "button", class: "lease-wizard-option", onclick: { let id = room.id as i64; let rent = crate::pricing_page::current_room_price(room); move |_| { choice_room.set(id); quick_rent.set(rent.map(|rent| rent.to_string()).unwrap_or_default()); show_lease_step(creation_step,5); } },
                                        span { class: "lease-wizard-option-icon", Icon { name: "house" } }
                                        span { strong { "{room.name}" } small { if let Some(rent) = crate::pricing_page::current_room_price(room) { "C${rent} / month" } else { "Set a monthly rent" } } }
                                    }
                                }
                            }
                        }
                    }
                    if creation_step() == 5 {
                        crate::collapsible_panel::FormCard { title: "Stay and rent".to_string(), class: Some("lease-stay-form-card".to_string()),
                            crate::app_form::AppForm { class: "lease-wizard-body lease-wizard-stay", onsubmit: move |event: FormEvent| event.prevent_default(),
                                crate::form_action_bar::TourPartySummary {
                                    image: selected_property_image.clone(), property_name: selected_property_name.clone(), property_subtitle: selected_room_name.clone(),
                                    guest_name: selected_guest_name.clone(), guest_email: choice_email(), guest_phone: selected_guest_phone.clone(),
                                }
                                crate::app_form::FormRows { class: Some("lease-stay-grid".to_string()),
                                    FormField { label: "Move-in date", required: true,
                                        DatePicker { label: "Move-in date", value: choice_start(), on_change: move |date| choice_start.set(date) }
                                    }
                                    FormField { label: "Guest email", required: true,
                                        if edit_contact_email() { crate::form_input::FormInput { r#type: "email", aria_label: "Guest email", value: "{choice_email}", placeholder: "guest@example.com", oninput: move |e: FormEvent| choice_email.set(e.value()) } }
                                        else { div { class: "lease-email-choice", strong { "{choice_email}" } button { r#type: "button", onclick: move |_| edit_contact_email.set(true), "Change" } } }
                                    }
                                }
                                div { class: "lease-duration-group",
                                    FormField { label: "Duration", required: true,
                                        div { class: "lease-duration-options", for months in [4, 8, 12, 24] { button { key: "{months}", r#type: "button", class: if !custom_duration() && choice_months() == months { "selected" } else { "" }, onclick: move |_| { choice_months.set(months); custom_duration.set(false); }, "{months} months" } } button { r#type: "button", class: if custom_duration() { "selected" } else { "" }, onclick: move |_| custom_duration.set(true), "Custom" } }
                                        if custom_duration() { FormField { label: "Length in months", required: true, crate::form_input::FormInput { r#type: "number", min: "1", max: "120", step: "1", value: "{custom_months}", oninput: move |e: FormEvent| custom_months.set(e.value()) } } }
                                    }
                                }
                                crate::app_form::FormRows { class: Some("lease-rent-row".to_string()),
                                    FormField { label: "Monthly rent (C$)", required: true,
                                        crate::form_input::FormInput { r#type: "number", min: "1", aria_label: "Monthly rent in Canadian dollars", value: "{quick_rent}", oninput: move |e: FormEvent| quick_rent.set(e.value()) }
                                    }
                                }
                                div { class: "lease-wizard-details", span { "Template" } strong { "{selected_template_name}" } }
                                div { class: "lease-wizard-details", span { "Landlord" } strong { "{selected_landlord_name}" } }
                                if stay_conflict { p { class: "lease-notice", role: "alert", "This rental unit already has a signed lease for these dates. Choose another unit or dates." } }
                                if email_configured() == Some(false) { p { class: "lease-notice", role: "alert", "Email sending is currently unavailable." } }
                                p { class: "lease-wizard-hint", "When every field is ready, use the check button above to review and email the lease." }
                            }
                        }
                    }
                }
            }
        }
        }
        if is_create_page && confirm_send() {
            div { class: "glass-dialog-backdrop", role: "dialog", aria_modal: "true", aria_label: "Review and email lease",
                div { class: "glass-dialog-card lease-review-dialog",
                    div { class: "glass-dialog-heading",
                        div { span { "FINAL CHECK" } h2 { "Ready to email this lease?" } }
                        button { class: "glass-icon-button", r#type: "button", aria_label: "Close review", title: "Close review", onclick: move |_| {confirm_send.set(false);slide_progress.set(0);}, Icon { name: "close" } }
                    }
                    crate::form_action_bar::PropertySummary {image:selected_property.map(|house|house.photos.first().cloned().unwrap_or_else(||crate::guest_pages::house_photo(house.id))).unwrap_or_default(),name:selected_property_name.clone(),subtitle:selected_room_name.clone()}
                    div { class: "lease-review-details",
                        div { class: "lease-review-person", span { "TO" } strong { "{selected_guest_name}" } small { "{choice_email}" } }
                        div { class: "lease-review-rent", span { "MONTHLY RENT" } strong { "C$ {quick_rent}" } small { "Due on the 1st" } }
                        crate::form_action_bar::StaySummary {start:choice_start(),months:chosen_months.unwrap_or(0)}
                    }
                    div { class: "lease-review-template", span { "TEMPLATE" } strong { "{selected_template_name}" } }
                    p { class: "lease-review-note", "The guest will receive a signing link by email." }
                    div { class: "glass-dialog-actions",
                        button { class: "glass-icon-button", r#type: "button", aria_label: "Go back", title: "Go back", onclick: move |_| {confirm_send.set(false);slide_progress.set(0);}, Icon { name: "arrow-left" } }
                        button { class: "glass-icon-button confirm desktop-confirm", r#type: "button", aria_label: "Confirm and email lease", title: "Confirm and email lease", disabled: busy(), onclick: move |_| send_ready.set(true), Icon { name: "check" } }
                    }
                    label { class: "lease-slide-confirm", span { "Slide to email lease" }
                        crate::form_input::FormInput { r#type: "range", min: "0", max: "100", value: "{slide_progress}", aria_label: "Slide to confirm email", disabled: busy(), oninput: move |event: FormEvent| {let progress=event.value().parse::<u32>().unwrap_or(0);slide_progress.set(progress);if progress>=95 {send_ready.set(true);}} }
                        span {class:"lease-slide-thumb",style:"left:{slide_left}",Icon { name: "chevron-right" }}
                    }
                }
            }
        }
        if !is_create_page && advanced() {div {class:"lease-editor-backdrop",role:"dialog",aria_modal:"true",aria_label:"Edit unsent lease",div {class:"lease-editor-panel",div {class:"lease-editor-head",div {p {class:"lease-eyebrow","FULL LEASE EDITOR"} h2 {"Edit unsent lease"}} button {r#type:"button",aria_label:"Close editor",onclick:move |_|advanced.set(false),Icon { name: "close" }}}
            section { class: "tenant-panel",
            crate::app_form::AppForm { class: "tenant-form lease-form", onsubmit: move |event: FormEvent| {event.prevent_default();busy.set(true);message.set(String::new());let whole_house=room_id()==-1;let body=serde_json::json!({"tenant_name":tenant_name(),"tenant_email":tenant_email(),"property_id":property_id(),"room_id":if whole_house{0}else{room_id()},"lease_type":if whole_house{"entire_house"}else{"room"},"details":details()});let id=editing_id();spawn(async move {let request=if id==0{Request::post("/api/leases")}else{Request::put(&format!("/api/leases/{id}"))};match request.json(&body).unwrap().send().await {Ok(r) if r.ok()=>{if let Ok(r)=Request::get("/api/leases").send().await {if let Ok(v)=r.json().await {leases.set(v)}}message.set("Unsent lease saved. Review it before emailing.".into())},Ok(r)=>message.set(error(r).await),Err(_)=>message.set("Could not save lease.".into())}busy.set(false);});},
                label { "Tenant full legal name *", crate::form_input::FormInput { required: true, value: "{tenant_name}", oninput: move |e: FormEvent| tenant_name.set(e.value()) } }
                label { "Tenant email *", crate::form_input::FormInput { r#type: "email", required: true, value: "{tenant_email}", oninput: move |e: FormEvent| tenant_email.set(e.value()) } }
                FormSelect { label: "Property", required: true, value: property_id().to_string(), placeholder: "Choose property", options: houses.iter().map(|h|(h.id.to_string(),format!("{} — {}",h.name,h.address))).collect::<Vec<_>>(), onchange: move |value:String| {property_id.set(value.parse().unwrap_or(0));room_id.set(0)} }
                FormSelect { label: "Rental unit", required: true, value: room_id().to_string(), placeholder: "Choose rental unit", options: std::iter::once(("-1".into(),"Entire house".into())).chain(rooms.iter().map(|r|(r.id.to_string(),r.name.clone()))).collect::<Vec<_>>(), onchange: move |value:String| room_id.set(value.parse().unwrap_or(0)) }
                label { "Unit description", crate::form_input::FormInput { value: "{d.unit_description}", oninput: move |e: FormEvent| details.write().unit_description=e.value() } }
                div { class: "date-field", span { "Tenancy start" } DatePicker { label: "Tenancy start", value: d.start_date.clone(), on_change: move |date| details.write().start_date=date } }
                div { class: "date-field", span { "Tenancy end, if fixed term" } DatePicker { label: "Tenancy end", value: d.end_date.clone(), on_change: move |date| details.write().end_date=date } }
                FormSelect { label: "Term type", required: false, value: d.term_type.clone(), placeholder: "Choose term", options: vec![("".into(),"Choose term".into()),("fixed".into(),"Fixed term".into()),("month-to-month".into(),"Month to month".into()),("other".into(),"Other".into())], onchange: move |value:String| details.write().term_type=value }
                if d.term_type=="other" { label { "Other term details", crate::form_input::FormInput { value: "{d.other_term}", oninput: move |e: FormEvent| details.write().other_term=e.value() } } }
                label { "Monthly rent ($) *", crate::form_input::FormInput { r#type: "number", min: "1", required: true, value: "{d.monthly_rent}", oninput: move |e: FormEvent| details.write().monthly_rent=e.value() } }
                label { "Rent due day of month *", crate::form_input::FormInput { r#type: "number", min: "1", max: "31", required: true, value: "{d.rent_due_day}", oninput: move |e: FormEvent| details.write().rent_due_day=e.value() } }
                label { "How rent is paid", crate::form_input::FormInput { value: "{d.rent_payment_method}", oninput: move |e: FormEvent| details.write().rent_payment_method=e.value() } }
                label { "Parking spaces and location", crate::form_input::FormInput { value: "{d.parking}", oninput: move |e: FormEvent| details.write().parking=e.value() } }
                FormSelect { label: "Condominium", required: false, value: d.condo.clone(), placeholder: "Choose", options: vec![("".into(),"Choose".into()),("No".into(),"No".into()),("Yes".into(),"Yes, rules supplied".into())], onchange: move |value:String| details.write().condo=value }
                label { class: "tenant-checkbox", crate::form_input::FormCheckbox { checked: d.email_notices, onchange: move |e: FormEvent| details.write().email_notices=e.checked() } "Landlord and tenant agree to receive notices by email where permitted" }
                if d.email_notices {
                    label { "Landlord email for notices", crate::form_input::FormInput { r#type:"email", maxlength:"90", value:"{d.landlord_notice_email}", oninput:move |e:FormEvent|details.write().landlord_notice_email=e.value() } }
                    label { "Landlord contact details", crate::form_input::FormInput { maxlength:"110", value:"{d.landlord_contact_details}", oninput:move |e:FormEvent|details.write().landlord_contact_details=e.value() } }
                }
                label { class: "tenant-checkbox", crate::form_input::FormCheckbox { checked: d.gas_included, onchange: move |e: FormEvent| details.write().gas_included=e.checked() } "Gas included" }
                label { class: "tenant-checkbox", crate::form_input::FormCheckbox { checked: d.ac_included, onchange: move |e: FormEvent| details.write().ac_included=e.checked() } "Air conditioning included" }
                label { class: "tenant-checkbox", crate::form_input::FormCheckbox { checked: d.storage_included, onchange: move |e: FormEvent| details.write().storage_included=e.checked() } "Storage included" }
                label { class: "tenant-checkbox", crate::form_input::FormCheckbox { checked: d.laundry_included, onchange: move |e: FormEvent| details.write().laundry_included=e.checked() } "On-site laundry included" }
                label { "Laundry charge, if any", crate::form_input::FormInput { value: "{d.laundry_fee}", oninput: move |e: FormEvent| details.write().laundry_fee=e.value() } }
                label { class: "tenant-checkbox", crate::form_input::FormCheckbox { checked: d.guest_parking_included, onchange: move |e: FormEvent| details.write().guest_parking_included=e.checked() } "Guest parking included" }
                label { "Services included in rent", crate::notebook_textarea::NotebookTextarea { label: "Services included in rent", value: d.services.clone(), onchange: move |text| details.write().services=text } }
                FormSelect { label: "Electricity paid by", required: false, value: d.electricity_payer.clone(), placeholder: "Choose", options: vec![("".into(),"Choose".into()),("landlord".into(),"Landlord".into()),("tenant".into(),"Tenant".into())], onchange: move |value:String| details.write().electricity_payer=value }
                FormSelect { label: "Heat paid by", required: false, value: d.heat_payer.clone(), placeholder: "Choose", options: vec![("".into(),"Choose".into()),("landlord".into(),"Landlord".into()),("tenant".into(),"Tenant".into())], onchange: move |value:String| details.write().heat_payer=value }
                FormSelect { label: "Water paid by", required: false, value: d.water_payer.clone(), placeholder: "Choose", options: vec![("".into(),"Choose".into()),("landlord".into(),"Landlord".into()),("tenant".into(),"Tenant".into())], onchange: move |value:String| details.write().water_payer=value }
                label { "Utility arrangement details", crate::notebook_textarea::NotebookTextarea { label: "Utility arrangement details", value: d.utilities.clone(), onchange: move |text| details.write().utilities=text } }
                label { "Rent discount (leave blank for none)", crate::notebook_textarea::NotebookTextarea { label: "Rent discount", value: d.rent_discount.clone(), onchange: move |text| details.write().rent_discount=text } }
                label { "Partial first rent, if applicable (amount and dates)", crate::notebook_textarea::NotebookTextarea { label: "Partial first rent", value: d.partial_rent.clone(), onchange: move |text| details.write().partial_rent=text } }
                label { "NSF administration charge ($0 to $20)", crate::form_input::FormInput { r#type: "number", min: "0", max: "20", value: "{d.nsf_fee}", oninput: move |e: FormEvent| details.write().nsf_fee=e.value() } }
                label { "Last month's rent deposit", crate::form_input::FormInput { value: "{d.rent_deposit}", oninput: move |e: FormEvent| details.write().rent_deposit=e.value() } }
                label { "Key deposit", crate::form_input::FormInput { value: "{d.key_deposit}", oninput: move |e: FormEvent| details.write().key_deposit=e.value() } }
                label { "Keys and devices covered by deposit", crate::notebook_textarea::NotebookTextarea { label: "Keys and devices covered by deposit", value: d.key_description.clone(), onchange: move |text| details.write().key_description=text } }
                label { "Smoking terms", crate::notebook_textarea::NotebookTextarea { label: "Smoking terms", value: d.smoking.clone(), onchange: move |text| details.write().smoking=text } }
                label { "Tenant insurance terms", crate::notebook_textarea::NotebookTextarea { label: "Tenant insurance terms", value: d.insurance.clone(), onchange: move |text| details.write().insurance=text } }
                label { "Other occupants", crate::notebook_textarea::NotebookTextarea { label: "Other occupants", value: d.occupants.clone(), onchange: move |text| details.write().occupants=text } }
                label { "Additional terms (attach only lawful terms)", crate::notebook_textarea::NotebookTextarea { label: "Additional terms", value: d.additional_terms.clone(), onchange: move |text| details.write().additional_terms=text } }
                label { "Other completed form details / notes", crate::notebook_textarea::NotebookTextarea { label: "Other completed form details and notes", value: d.notes.clone(), onchange: move |text| details.write().notes=text } }
                button { class: "primary-button", r#type: "submit", disabled: busy() || property_id()==0 || room_id()==0, if editing_id()==0 { "Save unsent lease" } else { "Update unsent lease" } }
            }
        }}}}
        }
    }
}

pub fn lease_templates_page() -> Element {
    let is_search_page = web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .is_some_and(|path| path == "/leases/templates/search");
    let is_editor = web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .is_some_and(|path| path.ends_with("/new"));
    let edit_id = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .and_then(|query| {
            query
                .strip_prefix("?id=")
                .and_then(|value| value.parse::<i64>().ok())
        })
        .unwrap_or(0);
    let mut templates = use_signal(Vec::<LeaseTemplate>::new);
    let mut properties = use_signal(Vec::<Property>::new);
    let mut name = use_signal(String::new);
    let mut landlord_id = use_signal(|| 0_i64);
    let mut is_landlord = use_signal(|| false);
    let mut landlords = use_signal(Vec::<Landlord>::new);
    let mut details = use_signal(Details::default);
    let mut template_step = use_signal(|| 1_u8);
    let mut confirm_template = use_signal(|| false);
    let saved_template_id = use_signal(|| edit_id);
    let mut template_search = use_signal(String::new);
    let mut message = use_signal(|| {
        if web_sys::window().and_then(|w| w.location().search().ok()).is_some_and(|q| q.contains("emailed=1")) {
            "Updated template emailed to the landlord.".to_string()
        } else { String::new() }
    });
    let mut busy = use_signal(|| false);
    let mut additional_pdf = use_signal(|| None::<Vec<u8>>);
    let mut additional_pdf_name = use_signal(String::new);
    let mut has_additional_pdf = use_signal(|| false);
    use_effect(move || {
        spawn(async move {
            if let Ok(r) = Request::get("/api/me").send().await {
                if let Ok(me) = r.json::<serde_json::Value>().await {
                    if me["role"] == "landlord" {
                        is_landlord.set(true);
                    }
                }
            }
            if let Ok(v) = api::list().await {
                properties.set(v)
            }
            if let Ok(r) = Request::get("/api/landlords").send().await {
                if let Ok(v) = r.json::<Vec<Landlord>>().await {
                    if is_landlord() {
                        landlord_id.set(v.first().map(|owner| owner.id).unwrap_or(0));
                    }
                    landlords.set(v)
                }
            }
            if let Ok(r) = Request::get("/api/lease-templates").send().await {
                if let Ok(v) = r.json::<Vec<LeaseTemplate>>().await {
                    if is_editor && edit_id > 0 {
                        if let Some(t) = v.iter().find(|t| t.id == edit_id) {
                            name.set(t.name.clone());
                            if !is_landlord() { landlord_id.set(t.landlord_id.unwrap_or(0)); }
                            details.set(t.details.clone());
                            has_additional_pdf.set(t.has_additional_pdf);
                        }
                    }
                    templates.set(v)
                }
            }
        });
    });
    let d = details();
    let _houses = properties();
    let owners = landlords();
    let landlord_email = owners.iter().find(|owner| owner.id == landlord_id())
        .map(|owner| owner.email.clone()).unwrap_or_default();
    let can_email_landlord = landlord_email.contains('@')
        && !landlord_email.to_ascii_lowercase().ends_with("@example.invalid");
    let template_term = template_search().to_lowercase();
    let visible_templates: Vec<_> = templates()
        .into_iter()
        .filter(|template| {
            template_term.is_empty()
                || format!(
                    "{} {}",
                    template.name,
                    template_landlord_name(&owners, template.landlord_id)
                )
                .to_lowercase()
                .contains(&template_term)
        })
        .collect();
    let no_matching_templates = visible_templates.is_empty();
    if is_search_page {
        return rsx! {
            document::Title { "Search templates | Buildry" }
            document::Stylesheet { href: crate::CSS }
            crate::admin_shell::AdminShell { active:"/leases", property_count:None, main_class:"main-content lease-main search-page-main",
                crate::searchable_list::SearchScreen { back_href:"/leases/templates", placeholder:"Search templates", value:template_search(), count:visible_templates.len(), on_search:move |value|template_search.set(value),
                    crate::searchable_list::JointList { class:"template-list",
                        div { class:"searchable-list-items",
                            if templates().is_empty() { p { class:"lease-list-empty", "No templates yet." } }
                            else if no_matching_templates { p { class:"lease-list-empty", "No templates match that search." } }
                            for template in visible_templates { a { key:"{template.id}", class:"template-row register-row", href:"/leases/templates/new?id={template.id}",
                                span { class:"lease-prospect-avatar", Icon { name:"file" } }
                                span { class:"lease-register-copy", strong { "{template.name}" } small { "{template_landlord_name(&owners,template.landlord_id)}" } }
                                span { class:"lease-register-status", "Template" }
                                Icon { name:"arrow-right" }
                            } }
                        }
                    }
                }
            }
        };
    }
    rsx! { Shell {title:if is_editor {"Create a lease template"}else{"Lease templates"},active:"/leases",
        if !is_editor {nav {class:"lease-section-nav",aria_label:"Lease sections",a {href:"/leases","Lease"} a {class:"active",href:"/leases/templates","Templates"}}}
        if !message().is_empty(){p {class:"lease-notice lease-toast",role:"status","{message}"}}
        if !is_editor {
            div {class:"lease-section-content",
            crate::searchable_list::JointList {class:"template-list",
                div {class:"searchable-list-items",
                if templates().is_empty(){div {class:"lease-register-empty",h3 {"No templates yet"}}}
                else if no_matching_templates {p {class:"lease-list-empty","No templates match your search."}}
                for template in visible_templates {a {key:"{template.id}",class:"template-row register-row",href:"/leases/templates/new?id={template.id}",
                    span {class:"lease-prospect-avatar",Icon {name:"file"}}
                    span {class:"lease-register-copy",strong {"{template.name}"} small {"{template_landlord_name(&owners,template.landlord_id)}"}}
                    span {class:"lease-register-status","Template"}
                    Icon {name:"arrow-right"}
                }}
                }
            }
            }
        } else {
            div {class:"template-editor-page",crate::form_action_bar::FormActionBar {back_href:"/leases/templates".to_string(),back_label:"Back to templates".to_string(),back_step:if template_step()>1 {Some(template_step)}else{None},title:Some(match template_step(){1=>"Template identity",2=>"Payment and property",3=>"Services and utilities",_=>"Deposits and terms"}.to_string()),
                button {class:"property-save-button",r#type:"button",aria_label:if template_step()==4 {"Review template"}else{"Next step"},title:if template_step()==4 {"Review template"}else{"Next step"},disabled:busy()||(template_step()==1&&(landlord_id()==0||name().trim().is_empty()))||(template_step()==2&&(d.rent_payment_method.trim().is_empty()||d.parking.trim().is_empty()||d.condo.is_empty()))||(template_step()==3&&(d.electricity_payer.is_empty()||d.heat_payer.is_empty()||d.water_payer.is_empty())),onclick:move |_|{if template_step()<4 {template_step.set(template_step()+1);if let Some(window)=web_sys::window(){window.scroll_to_with_x_and_y(0.0,0.0)}}else{confirm_template.set(true)}},if template_step()==4 {Icon {name:"check"}}else{Icon {name:"chevron-right"}}}
            }
                crate::form_action_bar::FormWizardHeader {eyebrow:"LEASE TEMPLATE".to_string(),title:match template_step(){1=>"Template identity",2=>"Payment and property",3=>"Services and utilities",_=>"Deposits and terms"}.to_string(),description:String::new(),step:template_step(),total:4}
                crate::app_form::AppForm {id:"template-editor-form",class:"template-form",onsubmit:move |event: FormEvent|event.prevent_default(),
                    if template_step()==1 {
                    crate::collapsible_panel::FormCard {title:"Template details".to_string(),class:Some("template-section".to_string()),
                        crate::app_form::FormRows {class:Some("template-fields".to_string()),FormField {label:"Template name",required:true,crate::form_input::FormInput {required:true,maxlength:"120",aria_label:"Template name",placeholder:"e.g. Standard furnished room",value:"{name}",oninput:move |e: FormEvent|name.set(e.value())}}
                            FormField {label:"Landlord",required:true,
                                if is_landlord() {
                                    p { class:"template-landlord-name", "{template_landlord_name(&owners, Some(landlord_id()))}" }
                                } else {
                                crate::form_multi_select::FormMultiSelect {
                                    label:"Landlord", class:"template-landlord-select", placeholder:"Choose a landlord", search_placeholder:"Search landlords", single_select:true,
                                    selected:if landlord_id()>0 {vec![landlord_id().to_string()]} else {vec![]},
                                    options:owners.iter().map(|owner| {let mut option=crate::form_multi_select::MultiSelectOption::new(owner.id.to_string(),owner.legal_name.clone(),owner.photo_url.clone());option.description=owner.email.clone();option}).collect(),
                                    onchange:move |values:Vec<String>|landlord_id.set(values.first().and_then(|value|value.parse().ok()).unwrap_or(0)),
                                }
                                }
                            }
                        }
                    }
                    }
                    if template_step()==2 {
                    crate::collapsible_panel::FormCard {title:"Payment details".to_string(),class:Some("template-section".to_string()),
                        crate::app_form::FormRows {class:Some("template-fields".to_string()),FormField {label:"How rent is paid",required:true,crate::form_input::FormInput {required:true,aria_label:"How rent is paid",placeholder:"e.g. E-transfer to the landlord",value:"{d.rent_payment_method}",oninput:move |e: FormEvent|details.write().rent_payment_method=e.value()}}
                            FormField {label:"Parking spaces and location",required:true,crate::form_input::FormInput {required:true,aria_label:"Parking spaces and location",placeholder:"e.g. No parking included",value:"{d.parking}",oninput:move |e: FormEvent|details.write().parking=e.value()}}
                            FormSelect {label:"Condominium",required:true,value:d.condo.clone(),placeholder:"Choose an answer",options:vec![("No".into(),"No".into()),("Yes".into(),"Yes, rules supplied".into())],onchange:move |value:String|details.write().condo=value}
                        }
                    }
                    section {class:"template-notice-card shared-form-card",
                        FormToggle {label:"Email notices where permitted",checked:d.email_notices,onchange:move |value:bool|details.write().email_notices=value}
                        if d.email_notices {
                            crate::app_form::FormRows {class:Some("template-fields".to_string()),
                                FormField {label:"Landlord email for notices",required:false,crate::form_input::FormInput {r#type:"email",aria_label:"Landlord email for notices",maxlength:"90",value:"{d.landlord_notice_email}",oninput:move |e:FormEvent|details.write().landlord_notice_email=e.value()}}
                                FormField {label:"Landlord contact details",required:false,crate::form_input::FormInput {aria_label:"Landlord contact details",maxlength:"110",value:"{d.landlord_contact_details}",oninput:move |e:FormEvent|details.write().landlord_contact_details=e.value()}}
                            }
                        }
                    }
                    }
                    if template_step()==3 {
                    section {class:"template-toggle-card",
                        div {class:"template-toggle-list",
                            FormToggle {label:"Gas included",checked:d.gas_included,onchange:move |value:bool|details.write().gas_included=value}
                            FormToggle {label:"Air conditioning included",checked:d.ac_included,onchange:move |value:bool|details.write().ac_included=value}
                            FormToggle {label:"Storage included",checked:d.storage_included,onchange:move |value:bool|details.write().storage_included=value}
                            FormToggle {label:"On-site laundry included",checked:d.laundry_included,onchange:move |value:bool|details.write().laundry_included=value}
                            FormToggle {label:"Guest parking included",checked:d.guest_parking_included,onchange:move |value:bool|details.write().guest_parking_included=value}
                        }
                    }
                    crate::collapsible_panel::FormCard {title:"Utilities".to_string(),class:Some("template-section".to_string()),
                        crate::app_form::FormRows {class:Some("template-fields".to_string()),
                            FormField {label:"Laundry charge, if any",required:false,div {class:"money-field",span {"C$"} crate::form_input::FormInput {r#type:"number",min:"0",aria_label:"Laundry charge",value:"{d.laundry_fee}",oninput:move |e: FormEvent|details.write().laundry_fee=e.value()}}}
                            FormField {label:"Services included in rent",required:false,crate::notebook_textarea::NotebookTextarea {label:"Services included in rent",value:d.services.clone(),onchange: move |text|details.write().services=text}}
                            FormSelect {label:"Electricity paid by",required:true,value:d.electricity_payer.clone(),placeholder:"Choose payer",options:vec![("landlord".into(),"Landlord".into()),("tenant".into(),"Tenant".into())],onchange:move |value:String|details.write().electricity_payer=value}
                            FormSelect {label:"Heat paid by",required:true,value:d.heat_payer.clone(),placeholder:"Choose payer",options:vec![("landlord".into(),"Landlord".into()),("tenant".into(),"Tenant".into())],onchange:move |value:String|details.write().heat_payer=value}
                            FormSelect {label:"Water paid by",required:true,value:d.water_payer.clone(),placeholder:"Choose payer",options:vec![("landlord".into(),"Landlord".into()),("tenant".into(),"Tenant".into())],onchange:move |value:String|details.write().water_payer=value}
                            FormField {label:"Utility arrangement details",required:false,crate::notebook_textarea::NotebookTextarea {label:"Utility arrangement details",value:d.utilities.clone(),onchange: move |text|details.write().utilities=text}}
                        }
                    }
                    }
                    if template_step()==4 {
                    crate::collapsible_panel::FormCard {title:"Deposits and terms".to_string(),class:Some("template-section".to_string()),
                        crate::app_form::FormRows {class:Some("template-fields".to_string()),
                            FormField {label:"NSF administration charge (C$0–20)",required:false,div {class:"money-field",span {"C$"} crate::form_input::FormInput {r#type:"number",min:"0",max:"20",aria_label:"NSF administration charge",value:"{d.nsf_fee}",oninput:move |e: FormEvent|details.write().nsf_fee=e.value()}}}
                            FormField {label:"Key deposit",required:false,div {class:"money-field",span {"C$"} crate::form_input::FormInput {r#type:"number",min:"0",aria_label:"Key deposit",value:"{d.key_deposit}",oninput:move |e: FormEvent|details.write().key_deposit=e.value()}}}
                            FormField {label:"Keys and devices covered by deposit",required:false,crate::notebook_textarea::NotebookTextarea {label:"Keys and devices covered by deposit",value:d.key_description.clone(),onchange: move |text|details.write().key_description=text}}
                            FormField {label:"Smoking terms",required:false,crate::notebook_textarea::NotebookTextarea {label:"Smoking terms",value:d.smoking.clone(),onchange: move |text|details.write().smoking=text}}
                            FormField {label:"Tenant insurance terms",required:false,crate::notebook_textarea::NotebookTextarea {label:"Tenant insurance terms",value:d.insurance.clone(),onchange: move |text|details.write().insurance=text}}
                            FormField {label:"Additional terms",required:false,crate::notebook_textarea::NotebookTextarea {label:"Additional terms",value:d.additional_terms.clone(),onchange: move |text|details.write().additional_terms=text}}
                            FormField {label:"Other form details and notes",required:false,crate::notebook_textarea::NotebookTextarea {label:"Other form details and notes",value:d.notes.clone(),onchange: move |text|details.write().notes=text}}
                            div { class:"template-pdf-pages",
                                div { class:"template-pdf-card",
                                    span { class:"template-pdf-icon", Icon {name:"file"} }
                                    div { class:"template-pdf-copy",
                                        strong { "Additional PDF pages" }
                                        if !additional_pdf_name().is_empty() { span { "Selected: {additional_pdf_name()}" } }
                                        else if has_additional_pdf() { span { "PDF attached to this template" } }
                                        else { span { "No additional PDF attached" } }
                                    }
                                    label { class:"template-pdf-pick",
                                        input { r#type:"file", accept:"application/pdf,.pdf", disabled:busy(), aria_label:"Choose additional PDF", onchange:move |event:FormEvent| {
                                            if let Some(file)=event.files().into_iter().next() {
                                                if file.size()>5_000_000 { message.set("The additional PDF must be smaller than 5 MB.".into()); return; }
                                                let filename=file.name();
                                                spawn(async move {
                                                    match file.read_bytes().await {
                                                        Ok(bytes)=>{additional_pdf.set(Some(bytes.to_vec()));additional_pdf_name.set(filename);message.set(String::new());},
                                                        Err(_)=>message.set("Could not read the additional PDF.".into()),
                                                    }
                                                });
                                            }
                                        } }
                                        span { if has_additional_pdf() || !additional_pdf_name().is_empty() { "Replace PDF" } else { "Choose PDF" } }
                                    }
                                }
                                p { "PDF only · up to 30 pages and 5 MB. These pages follow the standard lease and schedule." }
                                if !additional_pdf_name().is_empty() { button { r#type:"button", class:"template-pdf-remove", onclick:move |_|{additional_pdf.set(None);additional_pdf_name.set(String::new());}, "Clear selection" } }
                                else if has_additional_pdf() {
                                    button { r#type:"button", class:"template-pdf-remove", disabled:busy(), onclick:move |_| {
                                        let id=saved_template_id();
                                        if id<=0 {return;}
                                        busy.set(true);
                                        spawn(async move {
                                            match Request::delete(&format!("/api/lease-templates/{id}/additional-pdf")).send().await {
                                                Ok(response) if response.ok()=>{has_additional_pdf.set(false);message.set("Additional PDF removed.".into());},
                                                Ok(response)=>message.set(error(response).await),
                                                Err(_)=>message.set("Could not remove the additional PDF.".into()),
                                            }
                                            busy.set(false);
                                        });
                                    }, "Remove attached PDF" }
                                }
                            }
                        }
                    }
                    }
                    if template_step()<4 {div {class:"lease-wizard-footer",button {class:"primary-button",r#type:"button",disabled:busy()||(template_step()==1&&(landlord_id()==0||name().trim().is_empty()))||(template_step()==2&&(d.rent_payment_method.trim().is_empty()||d.parking.trim().is_empty()||d.condo.is_empty()))||(template_step()==3&&(d.electricity_payer.is_empty()||d.heat_payer.is_empty()||d.water_payer.is_empty())),onclick:move |_|{template_step.set(template_step()+1);if let Some(window)=web_sys::window(){window.scroll_to_with_x_and_y(0.0,0.0)}},"Continue",Icon {name:"chevron-right"}}}}
                }
                if confirm_template() {div {class:"glass-dialog-backdrop",role:"presentation",onclick:move |_|confirm_template.set(false),
                    div {class:"glass-dialog-card template-review-dialog",role:"dialog",aria_modal:"true",aria_label:"Review template delivery",onclick:move |event|event.stop_propagation(),
                        div {class:"glass-dialog-heading",div {span {"LEASE TEMPLATE"} h2 {"Review before sending"}} button {class:"back-icon-button",r#type:"button",aria_label:"Close",onclick:move |_|confirm_template.set(false),Icon {name:"close"}}}
                        p {class:"template-review-name","{name}"}
                        p {class:"template-review-landlord","Landlord: {template_landlord_name(&owners,Some(landlord_id()))}"}
                         p {class:"template-review-note","Save the template or email its updated PDF to the landlord. The PDF includes any additional pages selected here."}
                         if can_email_landlord { p {class:"template-review-landlord","Email to: {landlord_email}"} }
                         else { p {class:"template-review-note","Add a real email address to this landlord before emailing the template."} }
                        if !message().is_empty() {p {class:"lease-notice",role:"status","{message}"}}
                        div {class:"glass-dialog-actions",
                             button {class:"outline-button",r#type:"button",disabled:busy()||landlord_id()==0||name().trim().is_empty(),onclick:move |_|{busy.set(true);message.set(String::new());let body=serde_json::json!({"name":name(),"landlord_id":landlord_id(),"details":details()});let pdf=additional_pdf();spawn(async move {match save_template_with_pdf(saved_template_id(),body,pdf,false).await {Ok(_)=>{let _=web_sys::window().map(|w|w.location().set_href("/leases/templates"));},Err(reason)=>message.set(reason)}busy.set(false);});},"Save only"}
                             button {class:"primary-button",r#type:"button",disabled:busy()||!can_email_landlord||landlord_id()==0||name().trim().is_empty(),onclick:move |_|{busy.set(true);message.set(String::new());let body=serde_json::json!({"name":name(),"landlord_id":landlord_id(),"details":details()});let pdf=additional_pdf();spawn(async move {match save_template_with_pdf(saved_template_id(),body,pdf,true).await {Ok(_)=>{let _=web_sys::window().map(|w|w.location().set_href("/leases/templates?emailed=1"));},Err(reason)=>message.set(reason)}busy.set(false);});},"Save and email landlord"}
                        }
                    }
                }}
            }
        }
    }}
}
