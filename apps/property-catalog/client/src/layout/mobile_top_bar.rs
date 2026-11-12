use dioxus::prelude::*;
use crate::components::{icon::Icon, mcp_prompt_button::CopyMcpPromptButton};
use crate::{layout::sidebar, pages::tour_pages};

#[component]
pub(crate) fn MobileTopBar(path: String) -> Element {
    let is_calendar = matches!(path.as_str(), "/calendar" | "/tours");
    let can_compact = matches!(
        path.as_str(),
        "/" | "/properties" | "/calendar" | "/tours" | "/tasks" | "/leases" | "/leases/templates"
    );
    let (title, search, create) = match path.as_str() {
        "/" | "/properties" => (
            "Properties",
            Some(("/properties/search", "Search properties")),
            Some(("/properties/new", "Add property")),
        ),
        "/calendar" | "/tours" => ("Calendar", None, None),
        "/tasks" => (
            "Tasks",
            Some(("/tasks/search", "Search tasks")),
            Some(("/tasks/new", "Create task")),
        ),
        "/leases" => (
            "Lease Management",
            Some(("/leases/search", "Search leases")),
            Some(("/leases/new", "Create a lease")),
        ),
        "/leases/templates" => (
            "Lease Management",
            Some(("/leases/templates/search", "Search templates")),
            Some(("/leases/templates/new", "Create template")),
        ),
        _ => ("", None, None),
    };
    rsx! {
        header { class: if is_calendar { "mobile-app-topbar calendar-topbar can-compact" } else if title.is_empty() { "mobile-app-topbar is-hidden" } else if can_compact { "mobile-app-topbar can-compact" } else { "mobile-app-topbar" },
            if is_calendar {
                tour_pages::CalendarTopBar {}
            } else {
                h1 { "{title}" }
                if path == "/" || path=="/properties" {
                    CopyMcpPromptButton {}
                }
                if let Some((href, label)) = search {
                    a { class: "header-search-action", href, aria_label: label, title: label, Icon { name: "search" } }
                }
                if let Some((href, label)) = create.filter(|_| path != "/" || sidebar::can_create_resources()) {
                    a { class: "primary-button round-add-action", href, aria_label: label, title: label, Icon { name: "plus" } }
                }
            }
        }
    }
}
