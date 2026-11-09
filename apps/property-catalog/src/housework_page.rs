use crate::shared_ui::{ConfirmDialog, FilterBar, PageState, PageStateKind};
use crate::photo_gallery::{PhotoGallery, PhotoViewer};
use crate::schedule_summary::ScheduleSummary;
use crate::{
    api,
    form_button::{ButtonVariant, FormButton},
    form_action_bar::{FormActionBar, FormWizardHeader},
    form_select::FormSelect,
    housework_model::{HouseworkInput, HouseworkTicket},
    icon::Icon,
    model::Property,
};
use dioxus::prelude::*;
use gloo_net::http::Request;
use serde::Deserialize;

#[derive(Clone, Deserialize, PartialEq)]
struct AssigneeProfile {
    id: i64,
    first_name: String,
    last_name: String,
    role: String,
    active: bool,
    #[serde(default)]
    photo_url: String,
}

impl AssigneeProfile {
    fn name(&self) -> String {
        format!("{} {}", self.first_name.trim(), self.last_name.trim())
            .trim()
            .to_string()
    }
}

#[derive(Clone, PartialEq)]
struct FilterItem {
    value: String,
    label: String,
    image: String,
}

#[component]
fn HouseworkMultiPick(
    label: &'static str,
    icon: &'static str,
    items: Vec<FilterItem>,
    selected: Vec<String>,
    onchange: EventHandler<Vec<String>>,
    #[props(default)] portrait: bool,
) -> Element {
    rsx! { crate::form_multi_select::FormMultiSelect {
        label: label.to_string(), selected,
        options: items.into_iter().map(|item| crate::form_multi_select::MultiSelectOption::new(item.value, item.label, item.image)).collect(),
        onchange, class: if portrait { "housework-multipick portrait" } else { "housework-multipick" }, icon, placeholder: label,
    } }
}

#[component]
fn HouseworkAssigneeFilter(
    items: Vec<FilterItem>,
    value: String,
    onchange: EventHandler<String>,
) -> Element {
    let display = if value.is_empty() {
        "Assignee".to_string()
    } else {
        items
            .iter()
            .find(|item| item.value == value)
            .map(|item| item.label.clone())
            .unwrap_or_else(|| value.clone())
    };
    rsx! { FormSelect { label: "Assignee", required: false, value, placeholder: "Assignee", compact: true, display_override: display,
        options: std::iter::once((String::new(), "All assignees".into())).chain(items.iter().map(|item| (item.value.clone(), item.label.clone()))).collect(),
        show_avatars: true, avatar_initials_fallback: true,
        option_images: items.into_iter().map(|item| (item.value, item.image)).collect(),
        onchange: move |value: String| onchange.call(value),
    } }
}

#[component]
fn HouseworkStatusFilter(value: String, onchange: EventHandler<String>) -> Element {
    let display = if value == "all" {
        "Status".to_string()
    } else {
        status_label(&value).to_string()
    };
    rsx! { FormSelect { label: "Status", required: false, value, placeholder: "Status", compact: true, icon: "check", display_override: display,
        options: [("all","All statuses"),("backlog","Backlog"),("planned","Planned"),("in_progress","In progress"),("done","Done"),("aborted","Aborted")].into_iter().map(|(key,text)| (key.into(),text.into())).collect(),
        onchange: move |value: String| onchange.call(value),
    } }
}

async fn list() -> Result<Vec<HouseworkTicket>, String> {
    let response = Request::get("/api/tasks")
        .send()
        .await
        .map_err(|_| "Could not connect to tasks.".to_string())?;
    if !response.ok() {
        return Err(response
            .text()
            .await
            .unwrap_or_else(|_| "Could not load tasks.".into()));
    }
    response
        .json()
        .await
        .map_err(|_| "Could not read tasks.".into())
}

async fn save(id: Option<i64>, input: &HouseworkInput) -> Result<HouseworkTicket, String> {
    let request = if let Some(id) = id {
        Request::put(&format!("/api/tasks/{id}"))
    } else {
        Request::post("/api/tasks")
    };
    let response = request
        .json(input)
        .map_err(|_| "Could not prepare task.".to_string())?
        .send()
        .await
        .map_err(|_| "Could not connect to tasks.".to_string())?;
    if !response.ok() {
        return Err(response
            .text()
            .await
            .unwrap_or_else(|_| "Could not save task.".into()));
    }
    response
        .json()
        .await
        .map_err(|_| "Task saved, but the response could not be read.".into())
}

async fn delete_task(id: i64) -> Result<(), String> {
    let response = Request::delete(&format!("/api/tasks/{id}"))
        .send()
        .await
        .map_err(|_| "Could not connect to tasks.".to_string())?;
    if response.ok() {
        Ok(())
    } else {
        Err(response
            .text()
            .await
            .unwrap_or_else(|_| "Could not delete task.".into()))
    }
}

fn status_label(status: &str) -> &'static str {
    match status {
        "planned" => "Planned",
        "in_progress" => "In progress",
        "done" => "Done",
        "aborted" => "Aborted",
        _ => "Backlog",
    }
}

#[component]
fn TaskRows(
    items: Vec<HouseworkTicket>,
    properties: Vec<Property>,
    staff: Vec<AssigneeProfile>,
) -> Element {
    rsx! { div { class: "searchable-list-items task-list-items",
        for item in items { a { key: "{item.id}", class: "task-register-row register-row", href: "/tasks/{item.id}",
            span { class: "task-row-top",
                span { class: "housework-property",
                    span { class: "housework-house-avatar",
                        if let Some(photo) = properties.iter().find(|property| Some(i64::from(property.id)) == item.property_id).and_then(|property| property.photos.first()) { img { src: "{photo}", alt: "" } } else { Icon { name: "house" } }
                    }
                    if item.property_name.is_empty() { "General" } else { "{item.property_name}" }
                }
                span { class: format!("housework-status {}", item.status), "{status_label(&item.status)}" }
            }
            span { class: "task-row-main",
                span { class: "task-assignee-avatar", title: if item.assignee.is_empty() { "Unassigned" } else { item.assignee.as_str() }, aria_label: if item.assignee.is_empty() { "Unassigned" } else { item.assignee.as_str() },
                    if let Some(person) = staff.iter().find(|person| person.name() == item.assignee).filter(|person| !person.photo_url.is_empty()) { img { src: "{person.photo_url}", alt: "" } } else { Icon { name: "users" } }
                }
                span { class: "task-row-copy",
                    strong { "{item.title}" }
                    span { class: "task-row-meta",
                        span { if item.assignee.is_empty() { "Unassigned" } else { "{item.assignee}" } }
                        if let (Some(date), Some(time)) = (&item.start_date, &item.start_time) {
                            if matches!(item.status.as_str(), "planned" | "in_progress") {
                                span { class: "task-row-schedule", Icon { name: "calendar" } ScheduleSummary { date: date.clone(), time: time.clone() } }
                            }
                        }
                    }
                    small { class: "task-row-number", "TASK-{item.id}" if item.tenant_request { " · Tenant request" } }
                }
            }
        } }
    } }
}
pub fn page() -> Element {
    let path = web_sys::window()
        .and_then(|window| window.location().pathname().ok())
        .unwrap_or_default();
    let route_id = path
        .strip_prefix("/tasks/")
        .and_then(|id| id.parse::<i64>().ok());
    let is_search_page = path == "/tasks/search";
    let is_editor = path == "/tasks/new" || route_id.is_some();
    let mut tickets = use_signal(Vec::<HouseworkTicket>::new);
    let mut properties = use_signal(Vec::<Property>::new);
    let mut staff = use_signal(Vec::<AssigneeProfile>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let mut search = use_signal(|| {
        web_sys::window()
            .and_then(|window| window.location().search().ok())
            .and_then(|query| {
                query.trim_start_matches('?').split('&').find_map(|part| {
                    part.strip_prefix("ticket=")
                        .and_then(|value| value.parse::<i64>().ok())
                })
            })
            .map(|id| format!("TASK-{id}"))
            .unwrap_or_default()
    });
    let mut status_filter = use_signal(|| "all".to_string());
    let mut property_filter = use_signal(Vec::<String>::new);
    let mut assignee_filter = use_signal(String::new);
    let editing = use_signal(|| route_id);
    let form_open = use_signal(|| is_editor);
    let mut busy = use_signal(|| false);
    let mut uploading = use_signal(|| false);
    let mut viewing = use_signal(|| None::<usize>);
    let mut delete_confirm = use_signal(|| false);
    let mut pending_step = use_signal(|| None::<String>);
    let mut photos = use_signal(Vec::<String>::new);
    let mut creator_id = use_signal(|| None::<i64>);
    let mut tenant_request = use_signal(|| false);
    let mut can_work=use_signal(||route_id.is_none());
    let mut current_user_id = use_signal(|| None::<i64>);
    let mut title = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut property_id = use_signal(|| "".to_string());
    let mut assignee = use_signal(String::new);
    let mut status = use_signal(|| "backlog".to_string());
    let mut start_date = use_signal(String::new);
    let mut start_time = use_signal(String::new);
    let mut estimate = use_signal(|| "60".to_string());

    use_effect(move || {
        spawn(async move {
            match list().await {
                Ok(items) => {
                    if let Some(id) = route_id {
                        if let Some(item) = items.iter().find(|item| item.id == id) {
                            title.set(item.title.clone());
                            description.set(item.description.clone());
                            property_id.set(
                                item.property_id
                                    .map(|id| id.to_string())
                                    .unwrap_or_default(),
                            );
                            assignee.set(item.assignee_user_id.map(|id|id.to_string()).unwrap_or_default());
                            can_work.set(item.can_edit);
                            status.set(item.status.clone());
                            start_date.set(item.start_date.clone().unwrap_or_default());
                            start_time.set(item.start_time.clone().unwrap_or_default());
                            estimate.set(
                                item.estimate_minutes
                                    .map(|value| value.to_string())
                                    .unwrap_or_else(|| "60".into()),
                            );
                            photos.set(item.photos.clone());
                            creator_id.set(item.creator_id);
                            tenant_request.set(item.tenant_request);
                        } else {
                            error.set("Task not found.".into());
                        }
                    }
                    tickets.set(items)
                }
                Err(message) => error.set(message),
            }
            if let Ok(response) = Request::get("/api/tasks/properties").send().await {
                if let Ok(items) = response.json::<Vec<Property>>().await {
                    properties.set(items);
                }
            }
            if let Ok(response) = Request::get("/api/tasks/users").send().await {
                if let Ok(items) = response.json::<Vec<AssigneeProfile>>().await {
                    staff.set(items);
                }
            }
            if let Ok(response) = Request::get("/api/me").send().await {
                if let Ok(value) = response.json::<serde_json::Value>().await {
                    current_user_id.set(value["id"].as_i64());
                }
            }
            loading.set(false);
        });
    });
    let all = tickets();
    let term = search().to_lowercase();
    let mut visible: Vec<_> = all
        .iter()
        .filter(|item| {
            (term.is_empty()
                || format!(
                    "TASK-{} HW-{} {} {} {} {}",
                    item.id,
                    item.id,
                    item.title,
                    item.description,
                    item.property_name,
                    item.assignee
                )
                .to_lowercase()
                .contains(&term))
                && (status_filter() == "all" || item.status == status_filter())
                && (property_filter().is_empty()
                    || item
                        .property_id
                        .is_some_and(|id| property_filter().contains(&id.to_string())))
                && (assignee_filter().is_empty() || assignee_filter() == item.assignee)
        })
        .cloned()
        .collect();
    visible.sort_by_key(|item| -item.id);
    let mut assignees: Vec<_> = staff()
        .iter()
        .map(|user| FilterItem {
            value: user.name(),
            label: user.name(),
            image: user.photo_url.clone(),
        })
        .collect();
    for name in all
        .iter()
        .map(|item| item.assignee.clone())
        .filter(|name| !name.is_empty())
    {
        if !assignees.iter().any(|item| item.value == name) {
            assignees.push(FilterItem {
                value: name.clone(),
                label: name,
                image: String::new(),
            });
        }
    }
    assignees.sort_by(|a, b| a.label.cmp(&b.label));
    let houses: Vec<_> = properties()
        .iter()
        .map(|property| FilterItem {
            value: property.id.to_string(),
            label: property.name.clone(),
            image: property.photos.first().cloned().unwrap_or_default(),
        })
        .collect();
    let selected_house = properties()
        .into_iter()
        .find(|property| property.id.to_string() == property_id());
    let manager_options=std::iter::once((String::new(),"Unassigned".into())).chain(staff().iter().map(|m|(m.id.to_string(),m.name()))).collect::<Vec<_>>();
    let assignee_placeholder="Choose a contact";
    let can_edit = editing().is_none() || can_work();
    let locked = !can_edit || matches!(status().as_str(), "in_progress" | "done" | "aborted");
    let house_name = selected_house
        .as_ref()
        .map(|house| house.name.clone())
        .or_else(|| {
            tickets()
                .iter()
                .find(|ticket| Some(ticket.id) == editing())
                .map(|ticket| ticket.property_name.clone())
        })
        .unwrap_or_else(|| "General / no house".into());
    let wizard_step = match status().as_str() {
        "planned" => 2,
        "in_progress" => 3,
        "done" | "aborted" => 4,
        _ => 1,
    };
    let wizard_title = match status().as_str() {
        "planned" => "Plan the work",
        "in_progress" => "Work in progress",
        "done" => "Task complete",
        "aborted" => "Task aborted",
        _ => {
            if editing().is_some() {
                "Task backlog"
            } else {
                "Create a task"
            }
        }
    };
    let save_label = if busy() { "Saving…" } else if editing().is_none() { "Create task" } else {
        match status().as_str() {
            "backlog" => "Plan task",
            "planned" => "Start work",
            "in_progress" => "Complete task",
            _ => "Save task",
        }
    };
    if is_editor && (loading() || route_id.is_some_and(|id| !all.iter().any(|item| item.id == id))) {
        return rsx! {
            document::Title { "Task | Buildry" }
            main { id: "main-content", tabindex: "-1", class: "task-detail-page",
                div { class: "task-wizard-page lease-create-page",
                    FormActionBar { back_href: "/tasks".to_string(), back_label: "Back to tasks".to_string() }
                    if loading() { PageState { kind: PageStateKind::Loading, title: "Loading task…" } }
                    else {
                        PageState { kind: PageStateKind::Missing, page: true, title: if error() == "Task not found." { "Task not found" } else { "Task unavailable" },
                            if error() != "Task not found." { PageState { class: "housework-error", kind: PageStateKind::Error, title: error() } }
                            a { class: "primary-button", href: "/tasks", "Back to tasks" }
                        }
                    }
                }
            }
        };
    }
    let filters_active = !term.is_empty()
        || status_filter() != "all"
        || !property_filter().is_empty()
        || !assignee_filter().is_empty();

    if is_search_page {
        return rsx! {
            document::Title { "Search tasks | Buildry" }
            main { id: "main-content", tabindex: "-1", class: "main-content housework-main search-page-main task-search-page",
                crate::searchable_list::SearchScreen { back_href: "/tasks", placeholder: "Search tasks, houses, people…", value: search(), count: visible.len(), on_search: move |value| search.set(value),
                    FilterBar { class: "housework-filters task-standalone-filters", label: "Filter tasks", active: filters_active,
                        on_clear: move |_| { search.set(String::new()); status_filter.set("all".into()); property_filter.set(vec![]); assignee_filter.set(String::new()); },

                        HouseworkStatusFilter { value: status_filter(), onchange: move |value| status_filter.set(value) }
                        HouseworkMultiPick { label: "Houses", icon: "house", items: houses, selected: property_filter(), onchange: move |values| property_filter.set(values) }
                        HouseworkAssigneeFilter { items: assignees, value: assignee_filter(), onchange: move |value| assignee_filter.set(value) }
                    }
                    crate::searchable_list::JointList { class: "housework-board task-joint-list",
                        if !error().is_empty() { PageState { class: "housework-error", kind: PageStateKind::Error, title: error() } }
                        if loading() { PageState { class: "housework-empty", kind: PageStateKind::Loading, title: "Loading tasks…" } }
                        else if error().is_empty() && visible.is_empty() { PageState { class: "housework-empty", title: "No matching tasks" } }
                        else { TaskRows { items: visible, properties: properties(), staff: staff() } }
                    }
                }
            }
        };
    }

    rsx! {
        document::Title { "Tasks | Buildry" }
        main { id: if is_editor { "background-content" } else { "main-content" }, tabindex: "-1", class: if is_editor { "main-content housework-main editor-mode" } else { "main-content housework-main" },
            section { class: "page-head housework-head",
                h1 { "Tasks" }
                a { class: "primary-button housework-create", href: "/tasks/new", aria_label: "Create task", title: "Create task", Icon { name: "plus" } }
            }
            div { class: "housework-toolbar task-standalone-toolbar",
                div { class: "housework-search", Icon { name: "search" } crate::form_input::FormInput { r#type: "search", placeholder: "Search tasks, houses, people…", aria_label: "Search tasks", value: "{search}", oninput: move |event: FormEvent| search.set(event.value()) } }
                FilterBar { class: "housework-filters", label: "Filter tasks", active: filters_active,
                    on_clear: move |_| { search.set(String::new()); status_filter.set("all".into()); property_filter.set(vec![]); assignee_filter.set(String::new()); },

                    HouseworkStatusFilter { value: status_filter(), onchange: move |value| status_filter.set(value) }
                    HouseworkMultiPick { label: "Houses", icon: "house", items: houses, selected: property_filter(), onchange: move |values| property_filter.set(values) }
                    HouseworkAssigneeFilter { items: assignees, value: assignee_filter(), onchange: move |value| assignee_filter.set(value) }
                }
            }
            crate::searchable_list::JointList { class: "housework-board task-joint-list",
                if !error().is_empty() { PageState { class: "housework-error", kind: PageStateKind::Error, title: error() } }
                if loading() { PageState { class: "housework-empty", kind: PageStateKind::Loading, title: "Loading tasks…" } }
                else if error().is_empty() && visible.is_empty() { PageState { class: "housework-empty", title: if filters_active { "No matching tasks" } else { "No tasks yet" } } }
                else { TaskRows { items: visible, properties: properties(), staff: staff() } }
            }
        }
        if form_open() { main { id: "main-content", tabindex: "-1", class: "task-detail-page",
            div { class: "task-wizard-page lease-create-page",
                FormActionBar {
                    back_href: "/tasks".to_string(),
                    back_label: "Back to tasks".to_string(),
                    title: Some(wizard_title.to_string()),
                    if editing().is_some() && can_edit && current_user_id() == creator_id() {
                        FormButton { class: "task-delete-trigger", r#type: "button", aria_label: "Delete task", title: "Delete task", onclick: move |_| delete_confirm.set(true), Icon { name: "trash" } }
                    }
                    if can_edit && editing().is_some() && !matches!(status().as_str(), "done" | "aborted") {
                        FormButton { variant: ButtonVariant::Outline, class: "task-header-action", r#type: "submit", form: "task-wizard-form", disabled: busy() || uploading(), onclick: move |_| {
                            pending_step.set(None);
                            if let Some(card) = web_sys::window().and_then(|window| window.document()).and_then(|document| document.query_selector(".task-wizard-card").ok().flatten()) { let _ = card.set_attribute("open", ""); }
                        }, "Save" }
                    }
                    if can_edit { FormButton { variant: ButtonVariant::Primary, class: "task-header-action", r#type: "submit", form: "task-wizard-form", aria_label: save_label, title: save_label, disabled: busy() || uploading(), onclick: move |_| {
                        if let Some(card) = web_sys::window().and_then(|window| window.document()).and_then(|document| document.query_selector(".task-wizard-card").ok().flatten()) {
                            let _ = card.set_attribute("open", "");
                        }
                        pending_step.set(Some(match status().as_str() {
                            "backlog" => "planned",
                            "planned" => "in_progress",
                            "in_progress" => "done",
                            "done" => "done",
                            _ => "aborted",
                        }.into()));
                    }, "{save_label}" } }
                }
                if let Some(id)=editing().filter(|_|creator_id()==current_user_id()) { a{class:"outline-button",href:format!("/sharing/task/{id}"),"Share"} }
                FormWizardHeader {
                    eyebrow: if let Some(id) = editing() { format!("TASK-{id}") } else { "CREATE A TASK".to_string() },
                    title: wizard_title.to_string(),
                    description: String::new(),
                    step: wizard_step,
                    total: 4,
                }
                crate::collapsible_panel::FormCard { title: "Task details".to_string(), class: Some("lease-wizard-card task-wizard-card".to_string()),
                    crate::app_form::AppForm { id: "task-wizard-form", class: "lease-wizard-body task-wizard-body", onsubmit: move |event: FormEvent| {
                        event.prevent_default();
                        if busy() || uploading() { return; }
                        let desired = pending_step().unwrap_or_else(|| status());
                        pending_step.set(None);
                        let scheduled = matches!(desired.as_str(), "planned" | "in_progress" | "done");
                        let input = HouseworkInput {
                            title: title(), description: description(), property_id: property_id().parse().ok(),
                            assignee: assignee(), status: desired,
                            start_date: (scheduled && !start_date().is_empty()).then(|| start_date()),
                            start_time: (scheduled && !start_time().is_empty()).then(|| start_time()),
                            estimate_minutes: if scheduled && !start_date().is_empty() && !start_time().is_empty() { estimate().parse().ok() } else { None },
                            photos: photos(),
                        };
                        if (matches!(input.status.as_str(), "in_progress" | "done") || input.start_date.is_some() || input.start_time.is_some())
                            && (input.start_date.is_none() || input.start_time.is_none() || input.estimate_minutes.is_none()) {
                            error.set("Set a start date, time and estimate before continuing.".into());
                            return;
                        }
                        busy.set(true);
                        error.set(String::new());
                        notice.set(String::new());
                        let id = editing();
                        spawn(async move {
                            match save(id, &input).await {
                                Ok(saved) => {
                                    if id.is_none() {
                                        if let Some(window) = web_sys::window() {
                                            let _ = window.location().set_href(&format!("/tasks/{}", saved.id));
                                        }
                                    } else {
                                        status.set(saved.status.clone());
                                        notice.set("Saved.".into());
                                        let mut current = tickets();
                                        if let Some(item) = current.iter_mut().find(|item| item.id == saved.id) { *item = saved; }
                                        tickets.set(current);
                                    }
                                }
                                Err(message) => error.set(message),
                            }
                            busy.set(false);
                        });
                    },
                        if !error().is_empty() { PageState { class: "housework-error", kind: PageStateKind::Error, title: error() } }
                        if !notice().is_empty() { p { class: "task-save-notice", role: "status", "{notice}" } }
                        if locked {
                            crate::app_form::FormRows {
                                crate::form_select::FormValue { label: "Title", value: title() }
                                crate::form_select::FormValue { label: "House", value: house_name.clone() }
                                if !description().is_empty() { crate::form_select::FormValue { label: "Description", value: description(), multiline: true } }
                            }
                        } else {
                            crate::app_form::FormRows { class: Some("task-wizard-fields".to_string()),
                                crate::form_select::FormField { label: "Title", required: true, crate::form_input::FormInput { aria_label: "Title", required: true, maxlength: "180", value: "{title}", placeholder: "What needs to be done?", oninput: move |event: FormEvent| title.set(event.value()) } }
                                crate::form_select::FormField { label: "Description", required: false, crate::notebook_textarea::NotebookTextarea { label: "Description", value: description(), placeholder: "Add steps, context or access notes…", maxlength: 10000, onchange: move |text| description.set(text) } }
                                if tenant_request() { crate::form_select::FormValue { label: "House", value: house_name.clone() } }
                                else { FormSelect { label: "Property", required: false, value: property_id(), placeholder: "Choose a house", options: std::iter::once((String::new(),"None".into())).chain(properties().iter().map(|property| (property.id.to_string(), property.name.clone()))).collect(), show_avatars: true, option_images: properties().iter().map(|property| (property.id.to_string(), property.photos.first().cloned().unwrap_or_default())).collect(), onchange: move |value| { property_id.set(value); assignee.set(String::new()); } } }
                            }
                        }
                        if can_edit && !matches!(status().as_str(), "done" | "aborted") {
                            crate::app_form::FormRows { class: Some("task-wizard-assignee".to_string()),
                                FormSelect { label: "Assignee", required: false, value: assignee(), placeholder: assignee_placeholder, disabled: !can_edit || (editing().is_some() && creator_id()!=current_user_id()), options: manager_options, show_avatars: true, avatar_initials_fallback: true, option_images: staff().iter().map(|manager| (manager.id.to_string(), manager.photo_url.clone())).collect(), onchange: move |value| assignee.set(value) }
                            }
                        } else {
                            crate::app_form::FormRows {
                                crate::form_select::FormValue { label: "Assignee", value: if assignee().is_empty() { "None".to_string() } else { assignee() } }
                            }
                        }
                        if can_edit && status() == "planned" {
                            crate::app_form::FormRows { class: Some("task-wizard-time-fields".to_string()),
                                crate::form_select::FormField { label: "Start date", required: true, crate::date_picker::DatePicker { label: "Start date", value: start_date(), on_change: move |value| start_date.set(value) } }
                                crate::form_select::FormField { label: "Start time", required: true, crate::time_picker::TimePicker { label: "Start time", required: true, value: start_time(), on_change: move |value| start_time.set(value) } }
                                crate::form_select::FormField { label: "Estimate (minutes)", required: true, crate::form_input::FormInput { aria_label: "Estimate (minutes)", r#type: "number", required: true, min: "1", max: "10080", value: "{estimate}", oninput: move |event: FormEvent| estimate.set(event.value()) } }
                            }
                        }
                        if matches!(status().as_str(), "in_progress" | "done") {
                            crate::app_form::FormRows { class: Some("task-wizard-time-fields".to_string()),
                                crate::form_select::FormValue { label: "Start date", value: crate::schedule_summary::display_date(&start_date()) }
                                crate::form_select::FormValue { label: "Start time", value: crate::schedule_summary::display_time(&start_time()) }
                                crate::form_select::FormValue { label: "Estimate (minutes)", value: estimate() }
                            }
                        }
                        section { class: "housework-photo-section task-wizard-photos",
                            h3 { "Photos" }
                            if can_edit {
                                crate::upload_dropzone::UploadDropzone { aria_label: "Upload task photos", disabled: uploading() || photos().len() >= 24,
                                    on_files: move |files: Vec<dioxus::html::FileData>| {
                                        if uploading() || files.is_empty() { return; }
                                        uploading.set(true); error.set(String::new());
                                        spawn(async move {
                                            for file in files {
                                                if photos().len() >= 24 { error.set("Maximum 24 photos.".into()); break; }
                                                let name = file.name();
                                                match api::upload_task_image(file).await {
                                                    Ok(path) => photos.write().push(path),
                                                    Err(message) => error.set(format!("{name}: {message}")),
                                                }
                                            }
                                            uploading.set(false);
                                        });
                                    }
                                }
                            }
                            if photos().is_empty() {
                                if uploading() { p { role: "status", "Uploading…" } } else if !can_edit { p { "No photos" } }
                            } else {
                                PhotoGallery { photos: photos(), title: "Task image", item_label: "image", disabled: uploading(),
                                    on_open: move |index| viewing.set(Some(index)),
                                    on_remove: if can_edit { Some(EventHandler::new(move |index: usize| { let mut current = photos(); if index < current.len() { current.remove(index); photos.set(current); } })) } else { None },
                                }
                            }
                        }
                    }
                }
                if let Some(task_id)=editing() {crate::payments::TaskBills {task_id}}
            }
        } }
        if delete_confirm() {
            ConfirmDialog { title: "Delete this task?", confirm_label: if busy() { "Deleting…" } else { "Delete task" }, busy: busy(),
                on_close: move |_| delete_confirm.set(false),
                on_confirm: move |_| { if let Some(id) = editing() { busy.set(true); spawn(async move { match delete_task(id).await { Ok(()) => { if let Some(window) = web_sys::window() { let _ = window.location().set_href("/tasks"); } }, Err(message) => { error.set(message); delete_confirm.set(false); busy.set(false); } } }); } },
                p { "This task and its schedule will be permanently removed." }
            }
        }
        if let Some(selected) = viewing() { if !photos().is_empty() {
            PhotoViewer { photos: photos(), title: "Task images", selected, back_label: "Back to task", on_close: move |_| viewing.set(None) }
        } }
    }
}
