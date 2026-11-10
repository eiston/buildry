use crate::{
    app_form::{AppForm, FormRows},
    collapsible_panel::FormCard,
    searchable_list::{JointList, SearchableList},
    form_button::{ButtonVariant, FormButton},
    form_input::{FormCheckbox, FormInput},
    form_select::FormField,
    icon::Icon,
};
use dioxus::prelude::*;
use gloo_net::http::Request;
use serde_json::{json, Value};

pub static USER: GlobalSignal<Option<Value>> = Signal::global(|| None);
pub const APPS: &[(&str, &str, &str, &str)] = &[
    ("properties", "Properties", "/properties", "house"),
    ("pricing", "Pricing", "/pricing", "file"),
    ("vacancy", "Vacancy", "/vacancy", "calendar"),
    ("calendar", "Calendar", "/calendar", "calendar"),
    ("tasks", "Tasks", "/tasks", "check"),
    ("leases", "Lease Management", "/leases", "file"),
    ("rentals", "My Rentals", "/my-rentals", "house"),
    ("payments", "Payments", "/payments", "file"),
];
pub fn strings(value: &Value, key: &str) -> Vec<String> {
    value[key]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}
pub async fn get(path: &str) -> Result<Value, String> {
    let r = Request::get(path)
        .send()
        .await
        .map_err(|_| "Could not connect.".to_string())?;
    if !r.ok() {
        return Err(r.text().await.unwrap_or("Request failed.".into()));
    }
    r.json()
        .await
        .map_err(|_| "Could not read response.".into())
}
pub async fn change(method: &str, path: &str, body: Value) -> Result<(), String> {
    let request = match method {
        "DELETE" => Request::delete(path),
        "POST" => Request::post(path),
        _ => Request::put(path),
    };
    let r = request
        .json(&body)
        .map_err(|_| "Could not prepare request.".to_string())?
        .send()
        .await
        .map_err(|_| "Could not connect.".to_string())?;
    if !r.ok() {
        return Err(r.text().await.unwrap_or("Request failed.".into()));
    }
    Ok(())
}
pub async fn refresh_user() {
    if let Ok(value) = get("/api/me").await {
        USER.signal().set(Some(value));
    }
}
pub fn navigate(path: &str) {
    if let Some(w) = web_sys::window() {
        let _ = w.location().set_href(path);
    }
}
fn toggle(mut values: Vec<String>, key: &str) -> Vec<String> {
    if values.iter().any(|v| v == key) {
        values.retain(|v| v != key);
    } else {
        values.push(key.into());
    }
    values
}
pub fn starter_apps(interests: &[String]) -> Vec<String> {
    let mut ids = Vec::new();
    for interest in interests {
        let defaults: &[&str] = match interest.as_str() {
            "tenant" => &["rentals", "payments", "tasks"],
            "owner" => &[
                "properties",
                "pricing",
                "vacancy",
                "calendar",
                "tasks",
                "leases",
                "payments",
            ],
            "worker" => &["tasks", "calendar", "payments"],
            _ => &[],
        };
        for id in defaults {
            if !ids.iter().any(|v| v == id) {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

#[component]
pub fn Onboarding() -> Element {
    let mut interests = use_signal(Vec::<String>::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    rsx! {document::Title{"Welcome | Buildry"}main{class:"main-content workspace-onboarding",h1{"How will you use Buildry?"}
    AppForm{onsubmit:move|e:FormEvent|{e.prevent_default();busy.set(true);spawn(async move{let apps=starter_apps(&interests());match change("PUT","/api/account/preferences",json!({"interests":interests(),"installed_apps":apps})).await{Ok(())=>{refresh_user().await;let path=web_sys::window().and_then(|w|w.location().pathname().ok()).unwrap_or_default();if path=="/onboarding"||path=="/"{navigate(APPS.iter().find(|a|Some(a.0)==apps.first().map(String::as_str)).map(|a|a.2).unwrap_or("/apps"));}},Err(e)=>error.set(e)}busy.set(false);});},
    FormCard{title:"Interests",FormRows{for(id,label)in[("tenant","Tenant"),("owner","Property owner"),("worker","Worker")]{label{class:"workspace-choice",FormCheckbox{checked:interests().iter().any(|v|v==id),onchange:move|_|{interests.set(toggle(interests(),id));}},"{label}"}}}}
    if !error().is_empty(){p{role:"alert","{error}"}}FormButton{r#type:"submit",disabled:interests().is_empty(),busy:busy(),"Continue"}}
    AppForm{class:"workspace-onboarding-signout",method:"post",action:"/logout",FormButton{variant:ButtonVariant::Text,r#type:"submit","Sign out"}}}}
}

#[component]
pub fn AppsPage() -> Element {
    let mut query = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let installed = USER()
        .map(|u| strings(&u, "installed_apps"))
        .unwrap_or_default();
    rsx! {document::Title{"Apps | Buildry"}main{id:"main-content",class:"main-content workspace-main",div{class:"page-head",h1{"Apps"}}
    if !error().is_empty(){p{role:"alert","{error}"}}SearchableList{class:"workspace-app-list",search_placeholder:"Search apps",search_value:query(),on_search:move|value:String|query.set(value),
    if !APPS.iter().any(|a|a.1.to_lowercase().contains(&query().to_lowercase())){crate::shared_ui::PageState{title:"No apps found"}}
    for(id,name,path,icon)in APPS.iter().copied().filter(|a|a.1.to_lowercase().contains(&query().to_lowercase())){
    div{class:"workspace-app-row",Icon{name:icon}a{href:path,"{name}"}
    {let added=installed.iter().any(|v|v==id);rsx!{FormButton{variant:ButtonVariant::Outline,disabled:busy(),onclick:move|_|{busy.set(true);spawn(async move{if let Some(user)=USER(){let next=toggle(strings(&user,"installed_apps"),id);match change("PUT","/api/account/preferences",json!({"interests":strings(&user,"interests"),"installed_apps":next})).await{Ok(())=>refresh_user().await,Err(e)=>error.set(e)}}busy.set(false);});},if added{"Remove"}else{"Add"}}}}
    }}}}}
}

#[component]
pub fn AccountPage(path: String) -> Element {
    let user = USER().unwrap_or_default();
    let section = path.strip_prefix("/account/").unwrap_or("profile");
    let mut first = use_signal(|| user["first_name"].as_str().unwrap_or_default().to_string());
    let mut last = use_signal(|| user["last_name"].as_str().unwrap_or_default().to_string());
    let mut phone = use_signal(|| user["phone"].as_str().unwrap_or_default().to_string());
    let mut photo = use_signal(|| user["photo_url"].as_str().unwrap_or_default().to_string());
    let mut current = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut confirm = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut message = use_signal(String::new);
    let mut interests = use_signal(|| strings(&user, "interests"));
    let mut apps = use_signal(|| strings(&user, "installed_apps"));
    rsx! {document::Title{"Account | Buildry"}main{id:"main-content",class:"main-content workspace-main",div{class:"page-head",h1{"Account"}}
    nav{class:"workspace-tabs",aria_label:"Account settings",for(href,label,key)in[("/account","Profile","profile"),("/account/security","Security","security"),("/account/preferences","Preferences","preferences"),("/payments/settings","Payments","payments")]{a{href,aria_current:if section==key{"page"}else{"false"},"{label}"}}}
    if !message().is_empty(){p{class:"availability-message",role:"status","{message}"}}
    if section=="security"{
    if user["has_password"]==true{AppForm{onsubmit:move|e:FormEvent|{e.prevent_default();if password()!=confirm(){message.set("Passwords do not match.".into());return;}busy.set(true);spawn(async move{match change("POST","/api/change-password",json!({"current_password":current(),"new_password":password()})).await{Ok(())=>navigate("/login"),Err(e)=>message.set(e)}busy.set(false);});},
    FormCard{title:"Password",FormRows{FormField{label:"Current password",required:true,FormInput{r#type:"password",required:true,autocomplete:"current-password",value:current(),oninput:move|e:FormEvent|current.set(e.value())}}FormField{label:"New password",required:true,FormInput{r#type:"password",required:true,minlength:"12",autocomplete:"new-password",value:password(),oninput:move|e:FormEvent|password.set(e.value())}}FormField{label:"Confirm password",required:true,FormInput{r#type:"password",required:true,minlength:"12",autocomplete:"new-password",value:confirm(),oninput:move|e:FormEvent|confirm.set(e.value())}}}}
    FormButton{r#type:"submit",busy:busy(),"Update password"}}}else{a{class:"outline-button",href:"/forgot-password","Set password"}}
    div{class:"workspace-setting-row",span{"Google"}if user["google_connected"]==true{span{"Connected"}}else{a{href:"/auth/google/link","Connect"}}}
    FormButton{variant:ButtonVariant::Outline,disabled:busy(),onclick:move|_|{busy.set(true);spawn(async move{match change("POST","/api/account/sessions/revoke",json!({})).await{Ok(())=>navigate("/login"),Err(e)=>message.set(e)}busy.set(false);});},"Sign out of all devices"}
    }else if section=="preferences"{
    AppForm{onsubmit:move|e:FormEvent|{e.prevent_default();busy.set(true);spawn(async move{match change("PUT","/api/account/preferences",json!({"interests":interests(),"installed_apps":apps()})).await{Ok(())=>{refresh_user().await;message.set("Saved.".into());},Err(e)=>message.set(e)}busy.set(false);});},
    FormCard{title:"Interests",FormRows{for(id,label)in[("tenant","Tenant"),("owner","Property owner"),("worker","Worker")]{label{class:"workspace-choice",FormCheckbox{checked:interests().iter().any(|v|v==id),onchange:move|_|interests.set(toggle(interests(),id))},"{label}"}}}}
    FormCard{title:"Sidebar apps",class:"workspace-preferences-apps",for(index,id)in apps().iter().enumerate(){
    {let id=id.clone();let name=APPS.iter().find(|a|a.0==id).map(|a|a.1).unwrap_or("");rsx!{div{class:"workspace-app-row",span{"{name}"}FormButton{variant:ButtonVariant::Icon,r#type:"button",aria_label:format!("Move {name} up"),disabled:index==0,onclick:move|_|{let mut next=apps();next.swap(index,index-1);apps.set(next);},Icon{name:"chevron-up"}}FormButton{variant:ButtonVariant::Icon,r#type:"button",aria_label:format!("Move {name} down"),disabled:index+1>=apps().len(),onclick:move|_|{let mut next=apps();next.swap(index,index+1);apps.set(next);},Icon{name:"chevron-down"}}FormButton{variant:ButtonVariant::Icon,r#type:"button",aria_label:format!("Remove {name}"),onclick:move|_|apps.set(toggle(apps(),&id)),Icon{name:"close"}}}}}
    }
    }
    div{class:"workspace-actions",a{class:"outline-button",href:"/apps","Add apps"}FormButton{r#type:"submit",disabled:interests().is_empty(),busy:busy(),"Save preferences"}}}
    }else{
    AppForm{onsubmit:move|e:FormEvent|{e.prevent_default();busy.set(true);spawn(async move{match change("PUT","/api/account/profile",json!({"first_name":first(),"last_name":last(),"phone":phone(),"photo_url":if photo().starts_with("/images/"){Some(photo())}else{None}})).await{Ok(())=>{refresh_user().await;message.set("Saved.".into());},Err(e)=>message.set(e)}busy.set(false);});},FormCard{title:"Profile",FormRows{
    FormField{label:"Photo",required:false,
    if !photo().is_empty(){img{class:"workspace-avatar",src:photo(),alt:"Profile photo"}}
    crate::upload_dropzone::UploadDropzone{compact:true,multiple:false,label:"Change photo",hint:"",aria_label:"Change profile photo",disabled:busy(),on_files:move|files:Vec<dioxus::html::FileData>|{if let Some(file)=files.into_iter().next(){busy.set(true);spawn(async move{match crate::api::upload_task_image(file).await{Ok(url)=>photo.set(url),Err(e)=>message.set(e)}busy.set(false);});}}}
    }
    FormField{label:"First name",required:true,FormInput{required:true,maxlength:"60",autocomplete:"given-name",value:first(),oninput:move|e:FormEvent|first.set(e.value())}}
    FormField{label:"Last name",required:false,FormInput{maxlength:"60",autocomplete:"family-name",value:last(),oninput:move|e:FormEvent|last.set(e.value())}}
    crate::form_select::FormValue{label:"Email",value:user["email"].as_str().unwrap_or_default().to_string()}
    FormField{label:"Phone",required:false,FormInput{r#type:"tel",maxlength:"60",autocomplete:"tel",value:phone(),oninput:move|e:FormEvent|phone.set(e.value())}}}}
    FormButton{r#type:"submit",busy:busy(),"Save changes"}}
    }
    }}
}

#[component]
pub fn ContactsPage() -> Element {
    let mut contacts = use_signal(Vec::<Value>::new);
    let mut email = use_signal(String::new);
    let mut message = use_signal(String::new);
    let mut revision = use_signal(|| 0);
    let mut busy = use_signal(|| false);
    use_effect(move || {
        let _ = revision();
        spawn(async move {
            match get("/api/contacts").await {
                Ok(v) => contacts.set(v.as_array().cloned().unwrap_or_default()),
                Err(e) => message.set(e),
            }
        });
    });
    rsx! {document::Title{"Contacts | Buildry"}main{id:"main-content",class:"main-content workspace-main",div{class:"page-head",h1{"Contacts"}}
    AppForm{class:"workspace-inline-form",onsubmit:move|e:FormEvent|{e.prevent_default();busy.set(true);spawn(async move{match change("POST","/api/contacts",json!({"email":email()})).await{Ok(())=>{email.set(String::new());revision+=1;message.set("Request sent.".into());},Err(e)=>message.set(e)}busy.set(false);});},FormInput{r#type:"email",required:true,aria_label:"Contact email",placeholder:"Email",value:email(),oninput:move|e:FormEvent|email.set(e.value())}FormButton{r#type:"submit",busy:busy(),"Add contact"}}
    if !message().is_empty(){p{role:"status","{message}"}}
    JointList{class:"workspace-contact-list",for contact in contacts(){{let id=contact["id"].as_i64().unwrap_or(0);let accepted=contact["status"]=="accepted";rsx!{
    div{class:"workspace-contact-row",div{strong{{contact["name"].as_str().unwrap_or_default()}}small{{contact["email"].as_str().unwrap_or_default()}}}
    if !accepted && contact["incoming"]==true{FormButton{disabled:busy(),onclick:move|_|{busy.set(true);spawn(async move{match change("POST",&format!("/api/contacts/{id}/accept"),json!({})).await{Ok(())=>revision+=1,Err(e)=>message.set(e)}busy.set(false);});},"Accept"}}
    FormButton{variant:ButtonVariant::Text,disabled:busy(),onclick:move|_|{busy.set(true);spawn(async move{match change("DELETE",&format!("/api/contacts/{id}"),json!({})).await{Ok(())=>revision+=1,Err(e)=>message.set(e)}busy.set(false);});},if accepted{"Remove"}else if contact["incoming"]==true{"Decline"}else{"Cancel request"}}
    }}}}if contacts().is_empty(){crate::shared_ui::PageState{title:"No contacts"}}}
    }}
}

#[component]
pub fn RentalsPage() -> Element {
    let mut rentals = use_signal(|| None::<Vec<Value>>);
    let mut message = use_signal(String::new);
    use_effect(move || {
        spawn(async move {
            match get("/api/my-rentals").await {
                Ok(v) => rentals.set(Some(v.as_array().cloned().unwrap_or_default())),
                Err(e) => message.set(e),
            }
        });
    });
    rsx! {document::Title{"My Rentals | Buildry"}main{id:"main-content",class:"main-content workspace-main",div{class:"page-head",h1{"My Rentals"}}
    if !message().is_empty(){p{role:"alert","{message}"}}
    if let Some(items)=rentals(){if items.is_empty(){crate::shared_ui::PageState{title:"No rentals yet"}}
    for item in items{{let id=item["id"].as_i64().unwrap_or(0);let confirmed=item["status"]=="confirmed";rsx!{
    FormCard{class:"workspace-rental",title:item["property"].as_str().unwrap_or_default().to_string(),p{{item["unit"].as_str().unwrap_or_default()}}
    div{class:"workspace-setting-row",span{"Status"}strong{if item["ended"].is_string(){"Past rental"}else if confirmed{"Confirmed"}else if item["tenant_signed"]==true{"Awaiting confirmation"}else{"Awaiting your signature"}}}
    div{class:"workspace-setting-row",span{"Lease"}span{{format!("{} — {}",item["start"].as_str().unwrap_or_default(),item["end"].as_str().filter(|s|!s.is_empty()).unwrap_or("Ongoing"))}}}
    div{class:"workspace-setting-row",span{"Rent"}span{{format!("${} / month",item["rent"].as_str().unwrap_or_default())}}}
    div{class:"workspace-setting-row",span{"Landlord"}span{{item["landlord"].as_str().unwrap_or_default()}}}
    if !confirmed{div{class:"workspace-setting-row",span{"Identity document"}span{if item["id_approved"]==true{"Approved"}else if item["id_uploaded"]==true{"Uploaded"}else{"Required after signing"}}}}
    div{class:"workspace-actions",a{class:"outline-button",href:format!("/my-rentals/{id}/review"),if !item["tenant_signed"].as_bool().unwrap_or(false){"Review and sign"}else{"Review lease / ID"}}
    if item["tenant_signed"]==true{a{class:"outline-button",href:format!("/api/my-rentals/{id}/document"),target:"_blank",rel:"noopener","Lease document"}}
    if confirmed{a{class:"outline-button",href:"/payments","Payments"}a{class:"outline-button",href:format!("/my-home/requests/new?property_id={}",item["property_id"]),"Report an issue"}a{href:"/my-home/requests","Maintenance"}}
    }
    }}}}
    }else if message().is_empty(){p{role:"status","Loading rentals…"}}
    }}
}

#[component]
pub fn SignatureInbox() -> Element {
    let mut items = use_signal(Vec::<Value>::new);
    use_effect(move || {
        spawn(async move {
            if let Ok(v) = get("/api/my-signatures").await {
                items.set(v.as_array().cloned().unwrap_or_default());
            }
        });
    });
    rsx! {if !items().is_empty(){section{class:"shared-form-card workspace-signatures",for item in items(){a{class:"workspace-setting-row",href:format!("/leases/{}/sign",item["id"]),span{{item["tenant"].as_str().unwrap_or_default()}}span{if item["signed"]==true{"View signed lease"}else{"Your signature required"}}}}}}}
}

#[component]
pub fn SharingPage(kind: String, id: i64) -> Element {
    let mut contacts = use_signal(Vec::<Value>::new);
    let mut grants = use_signal(Vec::<Value>::new);
    let mut selected = use_signal(String::new);
    let mut permissions = use_signal(Vec::<String>::new);
    let mut message = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut revision = use_signal(|| 0);
    let kind_load = kind.clone();
    use_effect(move || {
        let _ = revision();
        let kind = kind_load.clone();
        spawn(async move {
            match get(&format!("/api/sharing/{kind}/{id}")).await {
                Ok(v) => grants.set(v.as_array().cloned().unwrap_or_default()),
                Err(e) => message.set(e),
            }
            if let Ok(v) = get("/api/contacts").await {
                contacts.set(
                    v.as_array()
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|v| v["status"] == "accepted")
                        .collect(),
                );
            }
        });
    });
    let options = contacts()
        .iter()
        .map(|v| {
            (
                v["user_id"].to_string(),
                v["name"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect::<Vec<_>>();
    let save_kind = kind.clone();
    rsx! {document::Title{"Sharing | Buildry"}main{id:"main-content",class:"main-content workspace-main",div{class:"page-head",h1{if kind=="task"{"Share task"}else{"Share property"}}}
    if !message().is_empty(){p{role:"status","{message}"}}
    AppForm{onsubmit:move|e:FormEvent|{e.prevent_default();let kind=save_kind.clone();busy.set(true);spawn(async move{match change("PUT",&format!("/api/sharing/{kind}/{id}"),json!({"user_id":selected().parse::<i64>().unwrap_or(0),"permissions":permissions()})).await{Ok(())=>{revision+=1;message.set("Saved.".into());},Err(e)=>message.set(e)}busy.set(false);});},
    FormCard{title:"Permissions",FormRows{
    crate::form_select::FormSelect{label:"Contact",required:true,placeholder:"Choose a contact",value:selected(),options,onchange:move|value:String|{permissions.set(grants().iter().find(|g|g["user_id"].to_string()==value).map(|g|strings(g,"permissions")).unwrap_or_default());selected.set(value);}}
    if kind=="task"{for(cap,label)in[("view","View"),("work","Work")]{label{class:"workspace-choice",FormCheckbox{checked:permissions().iter().any(|p|p==cap),onchange:move|_|permissions.set(toggle(permissions(),cap))},"{label}"}}}
    else{for(function,label)in[("details","Property details"),("pricing","Pricing & vacancy"),("tours","Tours"),("tasks","Tasks"),("leases","Leases"),("finances","Finances")]{div{class:"workspace-permission-row",span{"{label}"}for(action,label)in[("read","View"),("write","Edit")]{label{{let cap=format!("{function}.{action}");let checked=permissions().contains(&cap) || (action=="read" && permissions().contains(&format!("{function}.write")));rsx!{FormCheckbox{checked,onchange:move|event:FormEvent|{let mut next=permissions();next.retain(|v|v!=&cap);if event.checked(){next.push(cap.clone());if action=="write"{let view=format!("{function}.read");if !next.contains(&view){next.push(view);}}}else if action=="read"{next.retain(|v|v!=&format!("{function}.write"));}permissions.set(next);}}}}"{label}"}}}}
    label{class:"workspace-choice",FormCheckbox{checked:permissions().iter().any(|p|p=="leases.approve"),onchange:move|_|permissions.set(toggle(permissions(),"leases.approve"))},"Review ID & confirm tenancy"}}
    }}FormButton{r#type:"submit",disabled:selected().is_empty(),busy:busy(),"Save permissions"}}
    JointList{class:"workspace-grant-list",for grant in grants(){div{class:"workspace-setting-row",span{{grant["name"].as_str().unwrap_or_default()}}span{{strings(&grant,"permissions").join(", ")}}}}}
    }}
}

#[component]
pub fn LandlordsPage() -> Element {
    let mut profiles = use_signal(|| None::<Vec<Value>>);
    let mut message = use_signal(String::new);
    let path = web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .unwrap_or_default();
    let id = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .and_then(|q| {
            q.trim_start_matches('?')
                .split('&')
                .find_map(|p| p.strip_prefix("id=").and_then(|s| s.parse::<i64>().ok()))
        })
        .unwrap_or(0);
    use_effect(move || {
        spawn(async move {
            match get("/api/landlords").await {
                Ok(v) => profiles.set(Some(v.as_array().cloned().unwrap_or_default())),
                Err(e) => message.set(e),
            }
        });
    });
    rsx! {document::Title{"Landlord profiles | Buildry"}main{id:"main-content",class:"main-content workspace-main",div{class:"page-head",h1{if path.ends_with("/new"){"Add landlord profile"}else if path.ends_with("/view"){"Landlord profile"}else{"Landlord profiles"}}if !path.ends_with("/new") && !path.ends_with("/view"){a{class:"outline-button",href:"/landlords/new","Add profile"}}}
    if !message().is_empty(){p{role:"alert","{message}"}}
    if let Some(items)=profiles(){if path.ends_with("/new"){LegalProfileEditor{profile:json!({"id":0,"legal_name":"","service_address":"","email":"","phone":"","signer_name":"","signer_title":"","signature_authorized":false,"property_ids":[]})}}
    else if path.ends_with("/view"){if let Some(profile)=items.iter().find(|v|v["id"].as_i64()==Some(id)){LegalProfileEditor{key:"{id}",profile:profile.clone()}}else{crate::shared_ui::PageState{title:"Profile not found"}}}
    else{JointList{class:"workspace-profile-list",for profile in items{a{class:"workspace-setting-row",href:format!("/landlords/view?id={}",profile["id"]),span{{profile["legal_name"].as_str().unwrap_or_default()}}Icon{name:"chevron-right"}}}}}}
    else if message().is_empty(){p{role:"status","Loading profiles…"}}
    }}
}
#[component]
fn LegalProfileEditor(profile: Value) -> Element {
    let mut draft = use_signal(|| profile);
    let mut properties = use_signal(Vec::<Value>::new);
    let mut message = use_signal(String::new);
    let mut busy = use_signal(|| false);
    use_effect(move || {
        spawn(async move {
            if let Ok(v) = get("/api/properties?function=ownership").await {
                properties.set(v.as_array().cloned().unwrap_or_default());
            }
        });
    });
    rsx! {AppForm{onsubmit:move|e:FormEvent|{e.prevent_default();busy.set(true);spawn(async move{let id=draft()["id"].as_i64().unwrap_or(0);let path=if id==0{"/api/landlords".to_string()}else{format!("/api/landlords/{id}")};match change(if id==0{"POST"}else{"PUT"},&path,draft()).await{Ok(())=>navigate("/landlords"),Err(e)=>message.set(e)}busy.set(false);});},
    if !message().is_empty(){p{role:"alert","{message}"}}
    FormCard{title:"Landlord details",FormRows{for(key,label)in[("legal_name","Legal landlord name"),("service_address","Address for notices"),("email","Signer email"),("phone","Phone"),("signer_name","Designated signer"),("signer_title","Signer title")]{FormField{label,required:key!="phone",FormInput{r#type:if key=="email"{"email"}else{"text"},required:key!="phone",value:draft()[key].as_str().unwrap_or_default().to_string(),oninput:move|e:FormEvent|draft.write()[key]=json!(e.value())}}}
    label{class:"workspace-choice",FormCheckbox{checked:draft()["signature_authorized"]==true,onchange:move|e:FormEvent|draft.write()["signature_authorized"]=json!(e.checked())},"Authorized to sign for this landlord"}}}
    FormCard{title:"Properties",for property in properties(){{let id=property["id"].as_i64().unwrap_or(0);let name=property["name"].as_str().unwrap_or_default().to_string();rsx!{label{class:"workspace-choice",FormCheckbox{checked:draft()["property_ids"].as_array().is_some_and(|ids|ids.contains(&json!(id))),onchange:move|e:FormEvent|{let mut ids=draft()["property_ids"].as_array().cloned().unwrap_or_default();ids.retain(|v|*v!=json!(id));if e.checked(){ids.push(json!(id));}draft.write()["property_ids"]=json!(ids);}},"{name}"}}}}}
    FormButton{r#type:"submit",busy:busy(),"Save profile"}
    }}
}
