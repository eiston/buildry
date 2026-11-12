use dioxus::prelude::*;
use buildry_models::model::Property;
use crate::components::{icon::Icon, mcp_prompt_button::CopyMcpPromptButton, searchable_list};
use crate::{layout::sidebar, services::api, CSS, PORTFOLIO_MAP};

#[component]
pub(crate) fn CatalogPage() -> Element {
    return app(false);
}
#[component]
pub(crate) fn CatalogSearchPage() -> Element {
    return app(true);
}

fn app(is_search_page: bool) -> Element {
    let mut properties = use_signal(Vec::<Property>::new);
    let mut connection_status = use_signal(|| "Loading properties…".to_string());
    let mut search = use_signal(String::new);
    use_effect(move || sidebar::set_property_count(properties().len()));
    use_effect(move || {
        spawn(async move {
            let result = api::list().await;
            match result {
                Ok(saved) => {
                    properties.set(saved);
                    connection_status.set("Saved".into());
                }
                Err(error) => connection_status.set(error),
            }
        });
    });

    let list = properties();
    let query = search().to_lowercase();
    let visible: Vec<Property> = list
        .iter()
        .filter(|p| {
            p.name.to_lowercase().contains(&query)
                || p.address.to_lowercase().contains(&query)
                || p.id.to_string().contains(&query)
        })
        .cloned()
        .collect();
    let visible_is_empty = visible.is_empty();
    let pins: Vec<_> = list
        .iter()
        .filter_map(|property| {
            Some(serde_json::json!({
                "name": format!("House {} · {}", property.id, property.name),
                "address": property.address,
                "latitude": property.latitude?,
                "longitude": property.longitude?,
            }))
        })
        .collect();
    let portfolio_map_url = format!(
        "{}?pins={}",
        PORTFOLIO_MAP,
        urlencoding::encode(&serde_json::to_string(&pins).unwrap_or_default())
    );

    if is_search_page {
        return rsx! {
            document::Title { "Search properties | Buildry" }
            document::Stylesheet { href: CSS }
            crate::layout::admin_shell::AdminShell { active: "/properties", property_count: Some(list.len()), main_class: "main-content catalog-main search-page-main",
                searchable_list::SearchScreen { back_href: "/properties", placeholder: "Search properties", value: search(), count: visible.len(), on_search: move |value| search.set(value),
                    searchable_list::JointList { class: "catalog-panel",
                        div { class: "searchable-list-items property-list",
                            for property in visible {
                                searchable_list::ThumbnailListRow { key: "{property.id}", href: format!("/properties/{}", property.id), name: property.name.clone(), subtitle: property.location_label(), photo: property.photos.first().cloned() }
                            }
                            if visible_is_empty { p { class: "empty", "No properties match that search." } }
                        }
                    }
                }
            }
        };
    }

    rsx! {
        document::Title { "Properties | Buildry" }
        document::Stylesheet { href: CSS }
        crate::layout::admin_shell::AdminShell { active: "/properties", property_count: Some(list.len()), main_class: "main-content catalog-main",
            crate::components::shared_ui::PageHeader { class: "page-head",
                div {
                    h1 { "Properties" }
                }
                div { class: "page-actions",
                    CopyMcpPromptButton {}
                    a { class: "header-search-action", href: "/properties/search", aria_label: "Search properties", title: "Search properties", Icon { name: "search" } }
                    if sidebar::can_create_resources() { a { class: "primary-button round-add-action", href: "/properties/new", aria_label: "Add property", title: "Add property", Icon { name: "plus" } span { "Add property" } } }
                }
            }
            if connection_status() != "Saved" { p { class: "storage-status", "{connection_status}" } }
            section { class: "portfolio-map-panel",
                h2 { "Locations" }
                if pins.is_empty() {
                    div { class: "map-placeholder portfolio-map-empty", p { "No locations yet" } }
                } else {
                    iframe { class: "portfolio-map-frame", title: "Map of all property locations", src: portfolio_map_url }
                }
            }
            searchable_list::JointList { class: "catalog-panel",
                div { class: "searchable-list-items property-list",
                    for property in visible {
                        searchable_list::ThumbnailListRow { key: "{property.id}", href: format!("/properties/{}", property.id), name: property.name.clone(), subtitle: property.location_label(), photo: property.photos.first().cloned() }
                    }
                    if list.is_empty() { p { class: "empty", if sidebar::can_create_resources() { "No properties yet. Add your first house." } else { "No properties are assigned to this account." } } }
                    if !list.is_empty() && visible_is_empty { p { class: "empty", "No properties match that search." } }
                }
            }
        }
    }
}
