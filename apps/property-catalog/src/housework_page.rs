use crate::{
    api,
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
    let mut automatic = use_signal(|| false);
    let mut tenant_request = use_signal(|| false);
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
                            assignee.set(item.assignee.clone());
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
                            automatic.set(item.automation_run_id.is_some());
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
    use_effect(move || {
        if let Some(index) = viewing() {
            if let Some(element) = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| {
                    document.get_element_by_id(&format!("housework-viewer-photo-{index}"))
                })
            {
                element.scroll_into_view();
            }
        }
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
    let mut manager_options = Vec::new();
    for manager in staff().iter().filter(|manager| {
        manager.role == "staff" && {
            selected_house
                .as_ref()
                .is_some_and(|house| house.manager_staff_ids.contains(&manager.id))
        }
    }) {
        manager_options.push((manager.name(), manager.name()));
    }
    if let Some(house) = &selected_house {
        if house.owner_manages_tours && !house.owner_name.is_empty() {
            manager_options.push((house.owner_name.clone(), house.owner_name.clone()));
        }
    }
    if !assignee().is_empty() && !manager_options.iter().any(|(name, _)| *name == assignee()) {
        manager_options.push((assignee(), format!("{} · previous assignee", assignee())));
    }
    let assignee_placeholder: &'static str = if selected_house.is_some() {
        "Choose a house manager"
    } else {
        "Choose a house first"
    };
    let can_edit = editing().is_none()
        || current_user_id().is_some()
            && (current_user_id() == creator_id() || automatic() || tenant_request());
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
    let wizard_description = match status().as_str() {
        "planned" => "Set the schedule and assign a house manager before work starts.",
        "in_progress" => "Track the work and add photos as it moves toward completion.",
        "done" => "Review the completed task and add any final photos.",
        "aborted" => "This task was stopped. Its details and photos remain available.",
        _ => "Describe the work and choose the house and person responsible.",
    };
    let save_label = if busy() {
        "Saving task"
    } else if editing().is_none() {
        "Create task and continue"
    } else if matches!(status().as_str(), "done" | "aborted") {
        "Save task"
    } else {
        "Save and continue"
    };
    let filters_active = !term.is_empty()
        || status_filter() != "all"
        || !property_filter().is_empty()
        || !assignee_filter().is_empty();

    if is_search_page {
        return rsx! {
            document::Title { "Search tasks | Buildry" }
            main { class: "main-content housework-main search-page-main task-search-page",
                crate::searchable_list::SearchScreen { back_href: "/tasks", placeholder: "Search tasks, houses, people…", value: search(), count: visible.len(), on_search: move |value| search.set(value),
                    div { class: "housework-filters task-standalone-filters", aria_label: "Filter tasks",
                        if filters_active { button { class: "housework-clear", r#type: "button", onclick: move |_| { search.set(String::new()); status_filter.set("all".into()); property_filter.set(vec![]); assignee_filter.set(String::new()); }, "Clear" } }
                        HouseworkStatusFilter { value: status_filter(), onchange: move |value| status_filter.set(value) }
                        HouseworkMultiPick { label: "Houses", icon: "house", items: houses, selected: property_filter(), onchange: move |values| property_filter.set(values) }
                        HouseworkAssigneeFilter { items: assignees, value: assignee_filter(), onchange: move |value| assignee_filter.set(value) }
                    }
                    crate::searchable_list::JointList { class: "housework-board task-joint-list",
                        if !error().is_empty() { p { class: "housework-error", role: "alert", "{error}" } }
                        if loading() { p { class: "housework-empty", "Loading tasks…" } }
                        else if visible.is_empty() { div { class: "housework-empty", h2 { "No matching tasks" } p { "Try another search or filter." } } }
                        else { TaskRows { items: visible, properties: properties(), staff: staff() } }
                    }
                }
            }
        };
    }

    rsx! {
        document::Title { "Tasks | Buildry" }
        main { class: if is_editor { "main-content housework-main editor-mode" } else { "main-content housework-main" },
            section { class: "page-head housework-head",
                h1 { "Tasks" }
                a { class: "primary-button housework-create", href: "/tasks/new", aria_label: "Create task", title: "Create task", Icon { name: "plus" } }
            }
            div { class: "housework-toolbar task-standalone-toolbar",
                div { class: "housework-search", Icon { name: "search" } crate::form_input::FormInput { r#type: "search", placeholder: "Search tasks, houses, people…", aria_label: "Search tasks", value: "{search}", oninput: move |event: FormEvent| search.set(event.value()) } }
                div { class: "housework-filters",
                    if filters_active { button { class: "housework-clear", r#type: "button", onclick: move |_| { search.set(String::new()); status_filter.set("all".into()); property_filter.set(vec![]); assignee_filter.set(String::new()); }, "Clear" } }
                    HouseworkStatusFilter { value: status_filter(), onchange: move |value| status_filter.set(value) }
                    HouseworkMultiPick { label: "Houses", icon: "house", items: houses, selected: property_filter(), onchange: move |values| property_filter.set(values) }
                    HouseworkAssigneeFilter { items: assignees, value: assignee_filter(), onchange: move |value| assignee_filter.set(value) }
                }
            }
            crate::searchable_list::JointList { class: "housework-board task-joint-list",
                if !error().is_empty() { p { class: "housework-error", role: "alert", "{error}" } }
                if loading() { p { class: "housework-empty", "Loading tasks…" } }
                else if visible.is_empty() { div { class: "housework-empty", h2 { if filters_active { "No matching tasks" } else { "No tasks yet" } } p { if filters_active { "Try another filter." } else { "Create a task to start tracking work." } } } }
                else { TaskRows { items: visible, properties: properties(), staff: staff() } }
            }
        }
        if form_open() { main { class: "task-detail-page",
            div { class: "task-wizard-page lease-create-page",
                FormActionBar {
                    back_href: "/tasks".to_string(),
                    back_label: "Back to tasks".to_string(),
                    title: Some(wizard_title.to_string()),
                    if editing().is_some() && can_edit && current_user_id() == creator_id() {
                        button { class: "task-delete-trigger", r#type: "button", aria_label: "Delete task", title: "Delete task", onclick: move |_| delete_confirm.set(true), Icon { name: "trash" } }
                    }
                    if can_edit { button { class: "primary-button task-header-save", r#type: "submit", form: "task-wizard-form", aria_label: save_label, title: save_label, disabled: busy() || uploading(), onclick: move |_| {
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
                    }, Icon { name: "check" } } }
                }
                FormWizardHeader {
                    eyebrow: if let Some(id) = editing() { format!("TASK-{id}") } else { "CREATE A TASK".to_string() },
                    title: wizard_title.to_string(),
                    description: wizard_description.to_string(),
                    step: wizard_step,
                    total: 4,
                }
                if automatic() { span { class: "task-scheduled-note", "Scheduled task" } }
                crate::collapsible_panel::FormCard { title: "Task details".to_string(), class: Some("lease-wizard-card task-wizard-card".to_string()),
                    crate::app_form::AppForm { id: "task-wizard-form", class: "lease-wizard-body task-wizard-body", onsubmit: move |event: FormEvent| {
                        event.prevent_default();
                        if busy() || uploading() { return; }
                        let desired = pending_step().unwrap_or_else(|| status());
                        pending_step.set(None);
                        if desired != "aborted" && property_id().parse::<i64>().ok().filter(|id| *id > 0).is_none() {
                            error.set("Choose a house before continuing.".into());
                            return;
                        }
                        if desired != "aborted" && assignee().trim().is_empty() {
                            error.set("Choose a house manager before continuing.".into());
                            return;
                        }
                        let scheduled = matches!(desired.as_str(), "planned" | "in_progress" | "done");
                        let input = HouseworkInput {
                            title: title(), description: description(), property_id: property_id().parse().ok(),
                            assignee: assignee(), status: desired,
                            start_date: (scheduled && !start_date().is_empty()).then(|| start_date()),
                            start_time: (scheduled && !start_time().is_empty()).then(|| start_time()),
                            estimate_minutes: if scheduled && !start_date().is_empty() && !start_time().is_empty() { estimate().parse().ok() } else { None },
                            photos: photos(),
                        };
                        if matches!(input.status.as_str(), "in_progress" | "done")
                            && (input.start_date.is_none() || input.start_time.is_none() || input.estimate_minutes.is_none()) {
                            error.set("Set a start date, time and estimate before continuing.".into());
                            return;
                        }
                        busy.set(true);
                        error.set(String::new());
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
                        if !error().is_empty() { p { class: "housework-error", role: "alert", "{error}" } }
                        if locked {
                            div { class: "task-wizard-summary",
                                div { span { "TASK" } strong { "{title}" } }
                                div { span { "HOUSE" } strong { "{house_name}" } }
                                if !description().is_empty() { p { "{description}" } }
                            }
                        } else {
                            crate::app_form::FormRows { class: Some("task-wizard-fields".to_string()),
                                crate::form_select::FormField { label: "Title", required: true, crate::form_input::FormInput { aria_label: "Title", required: true, maxlength: "180", value: "{title}", placeholder: "What needs to be done?", oninput: move |event: FormEvent| title.set(event.value()) } }
                                crate::form_select::FormField { label: "Description", required: false, crate::notebook_textarea::NotebookTextarea { label: "Description", value: description(), placeholder: "Add steps, context or access notes…", maxlength: 10000, onchange: move |text| description.set(text) } }
                                if tenant_request() { div { class: "task-wizard-summary", div { span { "HOUSE" } strong { "{house_name}" } } } }
                                else { FormSelect { label: "House", required: true, value: property_id(), placeholder: "Choose a house", options: properties().iter().map(|property| (property.id.to_string(), property.name.clone())).collect(), show_avatars: true, option_images: properties().iter().map(|property| (property.id.to_string(), property.photos.first().cloned().unwrap_or_default())).collect(), onchange: move |value| { property_id.set(value); assignee.set(String::new()); } } }
                            }
                        }
                        if can_edit && !matches!(status().as_str(), "done" | "aborted") {
                            crate::app_form::FormRows { class: Some("task-wizard-assignee".to_string()),
                                FormSelect { label: "Assignee", required: true, value: assignee(), placeholder: assignee_placeholder, options: manager_options, show_avatars: true, avatar_initials_fallback: true, option_images: staff().iter().map(|manager| (manager.name(), manager.photo_url.clone())).collect(), onchange: move |value| assignee.set(value) }
                            }
                        } else {
                            div { class: "lease-wizard-details", span { "Assignee" } strong { class: if assignee().is_empty() { "empty-field-value" } else { "" }, if assignee().is_empty() { "None" } else { "{assignee}" } } }
                        }
                        if can_edit && status() == "planned" {
                            crate::app_form::FormRows { class: Some("task-wizard-time-fields".to_string()),
                                crate::form_select::FormField { label: "Start date", required: true, crate::form_input::FormInput { aria_label: "Start date", r#type: "date", required: true, value: "{start_date}", oninput: move |event: FormEvent| start_date.set(event.value()) } }
                                crate::form_select::FormField { label: "Start time", required: true, crate::form_input::FormInput { aria_label: "Start time", r#type: "time", required: true, value: "{start_time}", oninput: move |event: FormEvent| start_time.set(event.value()) } }
                                crate::form_select::FormField { label: "Estimate (minutes)", required: true, crate::form_input::FormInput { aria_label: "Estimate (minutes)", r#type: "number", required: true, min: "1", max: "10080", value: "{estimate}", oninput: move |event: FormEvent| estimate.set(event.value()) } }
                            }
                        }
                        if matches!(status().as_str(), "in_progress" | "done") {
                            div { class: "task-wizard-schedule-summary",
                                span { "SCHEDULE" }
                                strong { "{start_date} · {start_time} · {estimate} min" }
                            }
                        }
                        section { class: "housework-photo-section task-wizard-photos",
                            div { class: "gallery-heading",
                                div { p { class: "gallery-eyebrow", "IMAGE GALLERY" } p { class: "gallery-hint", if can_edit { "Add photos at any step" } else { "Task photos" } } }
                                if can_edit { label { class: "gallery-add", title: "Add images", aria_label: "Add images", Icon { name: "plus" }
                                    crate::form_input::FormFileInput { accept: "image/jpeg,image/png,image/webp", multiple: true, disabled: uploading() || photos().len() >= 24, onchange: move |event: FormEvent| {
                                        let files = event.files();
                                        if files.is_empty() { return; }
                                        uploading.set(true);
                                        spawn(async move {
                                            for file in files {
                                                if photos().len() >= 24 { break; }
                                                match api::upload_task_image(file).await {
                                                    Ok(path) => { let mut current = photos(); current.push(path); photos.set(current); }
                                                    Err(message) => { error.set(message); break; }
                                                }
                                            }
                                            uploading.set(false);
                                        });
                                    } }
                                } }
                            }
                            if photos().is_empty() {
                                div { class: "gallery-empty", Icon { name: "plus" } p { if uploading() { "Uploading images…" } else if can_edit { "Add images to this task" } else { "No task images" } } }
                            } else {
                                div { class: "gallery-grid", for (index, photo) in photos().iter().enumerate() {
                                    div { key: "{photo}", class: "gallery-photo",
                                        button { r#type: "button", class: "gallery-open", aria_label: "Open image {index + 1}", onclick: move |_| viewing.set(Some(index)), img { src: "{photo}", alt: "Task image {index + 1}" } }
                                        if can_edit { button { r#type: "button", class: "gallery-remove", aria_label: "Remove image {index + 1}", onclick: move |_| { let mut current = photos(); if index < current.len() { current.remove(index); photos.set(current); } }, Icon { name: "minus" } } }
                                    }
                                } }
                            }
                        }
                    }
                }
            }
        } }
        if delete_confirm() { div { class: "task-delete-overlay", role: "presentation", div { class: "task-delete-dialog", role: "alertdialog", aria_modal: "true", aria_label: "Delete task",
            h2 { "Delete this task?" }
            p { "This task and its schedule will be permanently removed." }
            div { button { r#type: "button", onclick: move |_| delete_confirm.set(false), "Cancel" }
                button { class: "task-delete-confirm", r#type: "button", disabled: busy(), onclick: move |_| { if let Some(id) = editing() { busy.set(true); spawn(async move { match delete_task(id).await { Ok(()) => { if let Some(window) = web_sys::window() { let _ = window.location().set_href("/tasks"); } }, Err(message) => { error.set(message); delete_confirm.set(false); busy.set(false); } } }); } }, "Delete task" }
            }
        } } }
        if viewing().is_some() && !photos().is_empty() { div { class: "gallery-viewer", role: "dialog", aria_modal: "true", aria_label: "Task images full view", div { class: "gallery-viewer-top", button { class: "gallery-back", r#type: "button", onclick: move |_| viewing.set(None), Icon { name: "arrow-left" } "Back to task" } span { "{photos().len()} images" } } div { class: "gallery-viewer-scroll", div { class: "gallery-viewer-stack", for (index, photo) in photos().iter().enumerate() { figure { id: "housework-viewer-photo-{index}", class: "gallery-viewer-photo", img { src: "{photo}", alt: "Task image {index + 1} of {photos().len()}" } figcaption { "{index + 1} of {photos().len()}" } } } } } } }
    }
}
