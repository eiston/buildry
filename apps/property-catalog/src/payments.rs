use crate::{
    app_form::{AppForm, FormRows},
    collapsible_panel::FormCard,
    date_picker::DatePicker,
    form_button::{ButtonVariant, FormButton},
    form_input::FormInput,
    form_select::FormField,
    icon::Icon,
    searchable_list::JointList,
};
use dioxus::prelude::*;
use gloo_net::http::Request;
use serde::Deserialize;

#[derive(Clone, Deserialize, PartialEq)]
struct Activity {
    id: String,
    kind: String,
    date: String,
    amount_cents: u64,
    status: String,
    direction: String,
    counterparty: String,
    property: String,
    invoice_url: Option<String>,
    payable: bool,
    #[serde(default)]
    invoice_number: String,
    due_date: Option<String>,
    source_url: Option<String>,
}
#[derive(Clone, Deserialize, PartialEq)]
struct Account {
    role: String,
    test_mode: bool,
    unavailable: bool,
    receive_status: String,
    #[serde(default)]
    today: String,
    rows: Vec<Activity>,
    leases: Vec<serde_json::Value>,
}
#[derive(Clone, Deserialize, PartialEq)]
struct Checkout {
    client_secret: String,
    publishable_key: String,
    stripe_account: String,
}
fn money(cents: u64) -> String {
    let whole = (cents / 100).to_string();
    let mut grouped = String::new();
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    format!("${grouped}.{:02}", cents % 100)
}
fn status(value: &str) -> &'static str {
    match value {
        "paid" | "recorded" => "Completed",
        "requested" => "Unpaid",
        "processing" => "Processing",
        "open" => "Open",
        "attention" => "Needs attention",
        "uncollectible" => "Uncollectible",
        "canceled" => "Canceled",
        _ => "Void",
    }
}
fn status_class(value: &str) -> &'static str {
    match value {
        "paid" | "recorded" => "payments-status completed",
        "canceled" | "void" => "payments-status canceled",
        "processing" => "payments-status processing",
        "attention" | "uncollectible" => "payments-status attention",
        _ => "payments-status",
    }
}
fn date(value: &str) -> String {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|d| d.format("%b %-d, %Y").to_string())
        .unwrap_or(value.into())
}
fn completed(row: &Activity) -> bool {
    matches!(row.status.as_str(), "paid" | "recorded")
}
fn query(name: &str) -> Option<String> {
    web_sys::window()?
        .location()
        .search()
        .ok()?
        .trim_start_matches('?')
        .split('&')
        .find_map(|s| s.strip_prefix(&format!("{name}=")).map(str::to_owned))
}
fn navigate(path: &str) {
    if let Some(w) = web_sys::window() {
        let _ = w.location().set_href(path);
    }
}
#[component]
pub fn PaymentsApp() -> Element {
    let route = use_signal(|| "/payments".to_string());
    use_context_provider(|| route);
    let mut account = use_signal(|| None::<Account>);
    let mut error = use_signal(String::new);
    let path = use_hook(|| {
        web_sys::window()
            .and_then(|w| w.location().pathname().ok())
            .unwrap_or("/payments".into())
    });
    use_effect(move || {
        spawn(async move {
            match Request::get("/api/payments").send().await {
                Ok(r) if r.ok() => {
                    if let Ok(value) = r.json::<Account>().await {
                        account.set(Some(value));
                        return;
                    }
                }
                _ => {}
            }
            error.set("Could not load account activity. Refresh to try again.".into());
        });
    });
    rsx! {
        document::Title{"Payments | Buildry"} document::Stylesheet{href:crate::CSS}
        div {class:"payments-app",
            main {id:"main-content",class:"payments-main",
                if let Some(data)=account(){PaymentContent{data,path}}
                else if !error().is_empty(){p{class:"tenant-request-error",role:"alert","{error}"}}
                else {p{class:"payment-history-message",role:"status","Loading account activity…"}}
            }
        }
    }
}
fn pending(row: &Activity) -> bool {
    matches!(
        row.status.as_str(),
        "requested" | "open" | "attention" | "processing"
    )
}
#[component]
fn PaymentContent(data: Account, path: String) -> Element {
    let title = match path.as_str() {
        "/payments/settings" => "Payment settings",
        "/payments/settings/payouts" => "Bank account",
        "/payments/settings/verification" => "Verification",
        "/payments/settings/autopay" => "Autopay",
        "/payments/settings/billing" => "Rent schedules",
        "/payments/bills/new" => "Create task bill",
        "/payments/bills" => "Bills",
        "/payments/bills/view" | "/payments/transaction" => "Bill",
        _ => "Account activity",
    };
    let bills_pending = data.rows.iter().any(pending);
    let settings_pending = data.receive_status == "pending";
    let back = if path == "/payments/settings/verification" {
        "/payments/settings/payouts"
    } else if path.starts_with("/payments/settings/") {
        "/payments/settings"
    } else if matches!(
        path.as_str(),
        "/payments/bills/new" | "/payments/bills/view" | "/payments/transaction"
    ) {
        "/payments/bills"
    } else {
        "/payments"
    };
    let back=if path=="/payments/bills/new" {query("task").and_then(|id|id.parse::<i64>().ok()).map(|id|format!("/tasks/{id}")).unwrap_or_else(||"/payments/bills".into())}else{back.to_string()};
    rsx! {
        div {class:"payments-page-head",
            div {class:"payments-heading",
                if path!="/payments" {a {class:"back-icon-button",href:back,aria_label:"Back",Icon {name:"arrow-left"}}}
                div {h1 {"{title}"}if data.test_mode&&!matches!(path.as_str(),"/payments/settings/payouts"|"/payments/settings/verification"|"/payments/settings/autopay") {span {class:"rent-badge payments-test-mode","Test mode"}}}
            }
            nav {class:"payments-utilities",aria_label:"Payment pages",
                a {class:if path.starts_with("/payments/settings"){"icon-button payments-utility active"}else{"icon-button payments-utility"},href:"/payments/settings",aria_label:"Payment settings",title:"Payment settings",Icon {name:"settings"}if settings_pending {span {class:"payments-pending-dot",aria_label:"Verification pending"}}}
                a {class:if path.starts_with("/payments/bills"){"icon-button payments-utility active"}else{"icon-button payments-utility"},href:"/payments/bills",aria_label:"Bills",title:"Bills",Icon {name:"file"}if bills_pending {span {class:"payments-pending-dot",aria_label:"Pending bills"}}}
            }
        }
        match path.as_str(){
            "/payments/settings"=>rsx!{PaymentSettings {data}},
            "/payments/settings/payouts"=>rsx!{crate::payout_setup::PayoutSetup {}},
            "/payments/settings/verification"=>rsx!{crate::payout_setup::PayoutSetup {verification:true}},
            "/payments/settings/autopay"=>rsx!{crate::tenants_page::AutopaySettings {}},
            "/payments/settings/billing"=>rsx!{BillingSettings {data}},
            "/payments/bills/new"=>rsx!{TaskBillCompose {data}},
            "/payments/bills"=>rsx!{BillsPage {data}},
            "/payments/bills/view"|"/payments/transaction"=>rsx!{TransactionDetail {data}},
            _=>rsx!{AccountActivity {data}}
        }
    }
}
#[component]
fn AccountActivity(data: Account) -> Element {
    let rows: Vec<Activity> = data
        .rows
        .iter()
        .filter(|r| {
            matches!(
                r.status.as_str(),
                "paid" | "recorded" | "processing" | "attention"
            )
        })
        .cloned()
        .collect();
    let mut dates = Vec::<String>::new();
    for row in &rows {
        if !dates.contains(&row.date) {
            dates.push(row.date.clone());
        }
    }
    rsx! {
        if data.unavailable {p {class:"tenant-request-error",role:"alert","Some payments could not be loaded. " a{href:"/payments","Retry"}}}
        if !rows.is_empty() || !data.unavailable {JointList {class:"payments-activity-list",
            if rows.is_empty(){div {class:"payments-empty",Icon {name:"wallet"}strong {"No transactions yet"}}}
            for day in dates {div {key:"{day}",class:"payments-day-group",h2 {class:"payments-day",if day==data.today {"Today"}else{"{date(&day)}"}}
                for row in rows.iter().filter(|row|row.date==day).cloned(){ActivityRow {key:"{row.id}",row,bill:false}}
            }}
        }}
    }
}
#[component]
fn BillsPage(data: Account) -> Element {
    let open: Vec<Activity> = data.rows.iter().filter(|r| pending(r)).cloned().collect();
    let settled: Vec<Activity> = data.rows.iter().filter(|r| completed(r)).cloned().collect();
    let closed: Vec<Activity> = data
        .rows
        .iter()
        .filter(|r| !pending(r) && !completed(r))
        .cloned()
        .collect();
    rsx! {
        if data.rows.is_empty(){JointList {class:"payments-activity-list",div {class:"payments-empty",Icon {name:"file"}strong {"No bills yet"}}}}
        for (heading,rows) in [("Outstanding",open),("Paid",settled),("Closed",closed)] {if !rows.is_empty(){section {class:"payments-bill-group",h2 {"{heading}"}JointList {class:"payments-activity-list",for row in rows {ActivityRow {key:"{row.id}",row,bill:true}}}}}}
    }
}
#[component]
fn ActivityRow(row: Activity, bill: bool) -> Element {
    let direction_icon: &'static str = if bill {
        "file"
    } else if row.direction == "in" {
        "arrow-down-left"
    } else {
        "arrow-up-right"
    };
    let context = if row.property.is_empty() {
        row.counterparty.clone()
    } else {
        format!("{} · {}", row.counterparty, row.property)
    };
    rsx! {a {class:"register-row payments-activity-row",href:"/payments/bills/view?id={row.id}",
        span {class:if row.direction=="in"{"payments-direction-icon incoming"}else{"payments-direction-icon"},Icon {name:direction_icon}}
        div {class:"payments-row-copy",strong {"{row.kind}"}span {"{context}"}if bill {small {if let Some(due)=row.due_date.clone() {"Due {date(&due)}"}else{"{date(&row.date)}"}}}}
        div {class:"payments-row-value",strong {class:if row.direction=="in"{"incoming"}else{""},if row.direction=="in"{"+"}else{"−"}"{money(row.amount_cents)}"}if bill||!completed(&row){span {class:status_class(&row.status),"{status(&row.status)}"}}}
        Icon {name:"chevron-right"}
    }}
}
#[component]
fn SettingLink(
    href: &'static str,
    title: &'static str,
    value: String,
    icon: &'static str,
) -> Element {
    rsx! {a{class:"register-row payments-setting-row",href,span{class:"payments-setting-icon",Icon{name:icon}}div{strong{"{title}"}if !value.is_empty(){span{"{value}"}}}Icon{name:"chevron-right"}}}
}
#[component]
fn PaymentSettings(data: Account) -> Element {
    rsx! {
        JointList{class:"payments-settings-list",
            SettingLink{href:"/payments/settings/payouts",title:"Bank account",value:match data.receive_status.as_str(){"ready"=>"Connected","pending"=>"Verification required",_=>"Set up"}.to_string(),icon:"wallet"}
            if data.leases.iter().any(|l|l["outgoing"]==true){SettingLink{href:"/payments/settings/autopay",title:"Autopay & payment methods",value:"".to_string(),icon:"calendar"}}
            SettingLink{href:"/payments/settings/billing",title:"Rent schedules",value:format!("{} rent schedule{}",data.leases.len(),if data.leases.len()==1{""}else{"s"}),icon:"file"}
        }
    }
}
#[component]
fn BillingSettings(data: Account) -> Element {
    rsx! {JointList{class:"payments-billing-list",
        if data.leases.is_empty(){p{class:"payment-history-message","No rent schedules"}}
        for lease in data.leases { BillingRow { lease } }
    }}
}
#[component]
fn BillingRow(lease: serde_json::Value) -> Element {
    let property = lease["property"].as_str().unwrap_or("Property");
    let counterparty = lease["counterparty"].as_str().unwrap_or("");
    let start = lease["start_date"].as_str().unwrap_or("");
    let end = lease["end_date"].as_str().unwrap_or("");
    let amount = money(lease["amount_cents"].as_u64().unwrap_or(0));
    let id = lease["id"].as_i64().unwrap_or(0);
    rsx! {div{class:"register-row payments-billing-row",span{class:"payments-direction-icon",Icon{name:"calendar"}}
        div{strong{"{property}"}span{"{counterparty}"}small{"{start} – {end}"}}
        div{class:"payments-row-value",strong{"{amount}"}small{"CAD / month"}}
        if lease["outgoing"]==true {a{href:"/payments/settings/autopay?lease={id}",aria_label:"Manage autopay",Icon{name:"chevron-right"}}}
    }}
}
#[component]
fn TaskBillCompose(data: Account) -> Element {
    let mut email = use_signal(String::new);
    let mut amount = use_signal(String::new);
    let mut due = use_signal(|| data.today.clone());
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut key = use_signal(String::new);
    use_effect(move || {
        if let Ok(crypto) = js_sys::Reflect::get(&js_sys::global(), &"crypto".into()) {
            if let Ok(function) = js_sys::Reflect::get(&crypto, &"randomUUID".into()) {
                if let Some(f) = function.dyn_ref::<js_sys::Function>() {
                    if let Ok(v) = f.call0(&crypto) {
                        key.set(v.as_string().unwrap_or_default());
                    }
                }
            }
        }
    });
    let task_id = use_hook(|| {
        query("task")
            .and_then(|id| id.parse::<i64>().ok())
            .unwrap_or(0)
    });
    let mut context = use_signal(|| None::<serde_json::Value>);
    use_effect(move || {
        spawn(async move {
            match Request::get(&format!("/api/tasks/{task_id}/bills"))
                .send()
                .await
            {
                Ok(r) if r.ok() => {
                    match r.json::<serde_json::Value>().await {
                        Ok(value) => context.set(Some(value)),
                        Err(_) => error.set("Could not load task bills.".into()),
                    }
                }
                Ok(r) if r.status() == 503 => error.set("Payments are temporarily unavailable.".into()),
                Ok(r) if r.status() == 404 => error.set("Task not found.".into()),
                _ => error.set("Could not load task bills.".into()),
            }
        });
    });
    let Some(task) = context() else {
        return rsx! {if error().is_empty(){p{role:"status","Loading task…"}}else{p{class:"tenant-request-error",role:"alert","{error}"}a{class:"outline-button",href:format!("/payments/bills/new?task={task_id}"),"Retry"}}};
    };
    if task["can_create"] != true {
        return rsx! {p{role:"alert","Canceled tasks cannot create bills."}};
    }
    let task_title = task["title"].as_str().unwrap_or("Task").to_string();
    let property = task["property"].as_str().unwrap_or("").to_string();
    rsx! {
        FormCard {title:task_title,class:"payments-compose-card",
            div{class:"payments-task-context",a{href:"/tasks/{task_id}","TASK-{task_id}"}span{"{property}"}}
            AppForm {onsubmit:move|event:FormEvent|{event.prevent_default();if busy(){return;}busy.set(true);error.set(String::new());let body=serde_json::json!({"key":key(),"email":email(),"amount":amount(),"due_date":due()});spawn(async move {match Request::post(&format!("/api/tasks/{task_id}/bills")).json(&body).unwrap().send().await{Ok(r) if r.ok()=>{if let Ok(value)=r.json::<serde_json::Value>().await{if let Some(id)=value["id"].as_str(){navigate(&format!("/payments/bills/view?id={id}"));return;}}},Ok(r)=>error.set(r.text().await.unwrap_or("Could not create bill.".into())),Err(_)=>error.set("Could not create bill. Try again.".into())}busy.set(false);});},
                FormRows {
                    FormField {label:"Recipient email",required:true,FormInput {r#type:"email",required:true,autocomplete:"email",placeholder:"Email address",value:email(),oninput:move|e:FormEvent|email.set(e.value())}}
                    FormField {label:"Amount (CAD)",required:true,FormInput {r#type:"number",inputmode:"decimal",min:"1",max:"999999.99",step:"0.01",required:true,placeholder:"0.00",value:amount(),oninput:move|e:FormEvent|amount.set(e.value())}}
                    FormField {label:"Due date",required:false,DatePicker {label:"Due date",value:due(),on_change:move|v|due.set(v)}}
                }
                if !error().is_empty(){p {class:"tenant-request-error",role:"alert","{error}"}}
                FormButton {r#type:"submit",variant:ButtonVariant::Primary,class:"payments-full-button",busy:busy(),disabled:key().is_empty(),"Create bill"}
            }
        }
    }
}
use wasm_bindgen::JsCast;
#[component]
fn TransactionDetail(data: Account) -> Element {
    let row = query("id")
        .and_then(|id| data.rows.iter().find(|r| r.id == id))
        .cloned();
    let mut checkout = use_signal(|| None::<Checkout>);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let Some(row) = row else {
        return rsx! {p{role:"alert","Bill not found."}};
    };
    let id = row.id.clone();
    let cancel_id = row.id.clone();
    let mut confirm_cancel = use_signal(|| false);
    let direction_icon: &'static str = if row.direction == "in" {
        "arrow-down-left"
    } else {
        "arrow-up-right"
    };
    rsx! {
        div{class:"payments-detail-layout",
            section{class:"payments-detail-summary",span{class:"payments-direction-icon",Icon{name:direction_icon}}h2{"{row.kind}"}strong{class:"payments-detail-amount","{money(row.amount_cents)}"}span{class:status_class(&row.status),"CAD · {status(&row.status)}"}
                dl{div{dt{"Invoice"}dd{"{row.invoice_number}"}}div{dt{if row.direction=="in"{"From"}else{"To"}}dd{"{row.counterparty}"}}if !row.property.is_empty(){div{dt{"Property"}dd{"{row.property}"}}}div{dt{"Issued"}dd{"{date(&row.date)}"}}if let Some(due)=row.due_date.clone(){div{dt{"Due"}dd{"{date(&due)}"}}}}
                if let Some(url)=row.source_url.clone(){a{class:"outline-button",href:url,if row.id.starts_with("lease-"){"View rent"}else{"View task"} Icon{name:"arrow-up-right"}}}
                if let Some(url)=row.invoice_url.clone(){a{class:"outline-button",href:url,target:"_blank",rel:"noopener noreferrer","Download invoice" Icon{name:"arrow-up-right"}}}
                if row.direction=="in" && row.status=="requested" && data.receive_status!="ready" {a {class:"outline-button payments-full-button",href:"/payments/settings/payouts","Set up payouts"}}
                if row.payable&&checkout().is_none(){FormButton{class:"payments-full-button",variant:ButtonVariant::Primary,busy:busy(),onclick:move|_|{busy.set(true);error.set(String::new());let id=id.clone();spawn(async move{match Request::post(&format!("/api/payments/{id}/checkout")).send().await{Ok(r) if r.ok()=>{if let Ok(value)=r.json::<Checkout>().await{checkout.set(Some(value));busy.set(false);return;}},Ok(r)=>error.set(r.text().await.unwrap_or("Could not start payment.".into())),Err(_)=>error.set("Could not start payment. Try again.".into())}busy.set(false);});},"Pay bill"}}
                if row.status=="requested" && row.payable && checkout().is_none() {
                    if confirm_cancel() {
                        FormButton {variant:ButtonVariant::Danger,class:"payments-full-button",busy:busy(),onclick:move|_|{busy.set(true);let id=cancel_id.clone();spawn(async move {match Request::post(&format!("/api/payments/{id}/cancel")).send().await {Ok(r) if r.ok()=>navigate("/payments/bills"),Ok(r)=>error.set(r.text().await.unwrap_or("Could not cancel request.".into())),Err(_)=>error.set("Could not cancel request. Try again.".into())}busy.set(false);});},"Confirm cancel bill"}
                        FormButton {variant:ButtonVariant::Text,onclick:move|_|confirm_cancel.set(false),"Keep bill"}
                    } else {FormButton {variant:ButtonVariant::Text,disabled:busy(),onclick:move|_|confirm_cancel.set(true),"Cancel bill"}}
                }
                if !error().is_empty(){p{class:"tenant-request-error",role:"alert","{error}"}}
            }
            if let Some(elements)=checkout(){
                document::Script{src:"/assets/account-payment.mjs?v=20261106-payments",r#type:"module"}
                section{class:"rent-card-entry payments-secure-entry","data-account-checkout":"{elements.client_secret}","data-stripe-key":"{elements.publishable_key}","data-stripe-account":"{elements.stripe_account}","data-amount":"{row.amount_cents}",
                    h2{"Payment method"}div{class:"account-payment-fields"}p{class:"account-payment-loading",role:"status","Loading secure fields…"}p{class:"account-payment-error tenant-request-error",role:"alert",hidden:true}
                    FormButton{class:"account-payment-submit payments-full-button",variant:ButtonVariant::Primary,disabled:true,"Pay {money(row.amount_cents)}"}
                }
            }
        }
    }
}

#[component]
pub fn TaskBills(task_id: i64) -> Element {
    let mut data = use_signal(|| None::<serde_json::Value>);
    use_effect(move || {
        spawn(async move {
            if let Ok(r) = Request::get(&format!("/api/tasks/{task_id}/bills"))
                .send()
                .await
            {
                if r.ok() {
                    if let Ok(v) = r.json::<serde_json::Value>().await {
                        data.set(Some(v));
                    }
                }
            }
        });
    });
    let Some(task) = data() else {
        return rsx! {};
    };
    let bills = task["bills"].as_array().cloned().unwrap_or_default();
    rsx! {FormCard{title:"Bills",class:"task-bills-card",
        if !bills.is_empty(){JointList{class:"payments-task-bills-list",for bill in bills{a{class:"register-row payments-task-bill-row",href:format!("/payments/bills/view?id={}",bill["id"].as_str().unwrap_or("")),strong{ {bill["number"].as_str().unwrap_or("")} }span{ {money(bill["amount_cents"].as_u64().unwrap_or(0))} }Icon{name:"chevron-right"}}}}}
        if task["can_create"]==true{a{class:"outline-button",href:"/payments/bills/new?task={task_id}",Icon{name:"plus"}"Create bill"}}
    }}
}
