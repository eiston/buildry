use crate::{api, model::Property, CSS};
use dioxus::prelude::*;

fn edit_rate(
    mut properties: Signal<Vec<Property>>,
    property_id: u32,
    space_id: u32,
    field: &str,
    value: String,
) {
    let mut list = properties.write();
    if let Some(space) = list
        .iter_mut()
        .find(|p| p.id == property_id)
        .and_then(|p| p.spaces.iter_mut().find(|s| s.id == space_id))
    {
        let rent = value.parse().ok();
        match field {
            "current" => {
                space.current_monthly_rent = rent;
                space.rent_is_estimate = false;
            }
            "sep" => space.rent_sep_dec = rent,
            "jan" => space.rent_jan_apr = rent,
            "may" => space.rent_may_aug = rent,
            _ => {}
        }
        if field != "current" {
            space.seasonal_prices_confirmed = false;
        }
    }
}

pub fn pricing_page() -> Element {
    let mut properties = use_signal(Vec::<Property>::new);
    let mut loading = use_signal(|| true);
    let mut saving = use_signal(|| None::<u32>);
    let mut message = use_signal(String::new);
    use_effect(move || {
        spawn(async move {
            match api::list().await {
                Ok(list) => properties.set(list),
                Err(error) => message.set(error),
            }
            loading.set(false);
        });
    });
    rsx! {
        document::Title { "Property pricing | Buildry" }
        document::Stylesheet { href: CSS }
        div { class: "app-shell",
            aside { class: "sidebar",
                div { class: "sidebar-header", div { class: "brand", span { class: "brand-mark", "B" } span { class: "brand-name", "buildry" } } }
                a { class: "nav-item tour-nav-link", href: "/", title: "Properties", span { class: "nav-icon icon-properties" } span { class: "nav-text", "Properties" } }
                a { class: "nav-item active", href: "/pricing", title: "Pricing", span { class: "nav-icon icon-pricing" } span { class: "nav-text", "Pricing" } }
                a { class: "nav-item tour-nav-link", href: "/tours", title: "House tours", span { class: "nav-icon icon-tours" } span { class: "nav-text", "House tours" } }
                a { class: "nav-item tour-nav-link", href: "/availability", title: "Availability", span { class: "nav-icon icon-availability" } span { class: "nav-text", "Availability" } }
                a { class: "nav-item tour-nav-link", href: "/stays", title: "Guest stays", span { class: "nav-icon icon-stays" } span { class: "nav-text", "Guest stays" } }
                form { class: "nav-logout", method: "post", action: "/logout", button { class: "nav-item tour-nav-link", r#type: "submit", title: "Sign out", span { class: "nav-icon icon-logout" } span { class: "nav-text", "Sign out" } } }
            }
            main { class: "main-content pricing-admin",
                section { class: "page-head", div { p { class: "eyebrow", "All properties" } h1 { "Pricing" } p { class: "catalog-summary", "Review and adjust every room's seasonal rates in one place." } } }
                p { class: "management-help", "Save each house after editing. Guest quotes use seasonal rates; September must be higher than January, and January higher than May." }
                if loading() { p { class: "stays-state", "Loading prices…" } }
                if !message().is_empty() { p { class: "availability-message", role: "status", "{message}" } }
                for property in properties() {
                    section { key: "{property.id}", class: "management-card pricing-house",
                        div { class: "management-card-head",
                            div { h2 { "House {property.id} · {property.name}" } p { class: "management-help", if property.address.trim().is_empty() { "Address not set" } else { "{property.address}" } } }
                            div { class: "page-actions", a { class: "outline-button", href: "/properties/{property.id}", "Manage property" }
                                button { class: "primary-button", disabled: saving().is_some(), onclick: { let id = property.id; move |_| {
                                    let Some(current) = properties().into_iter().find(|p| p.id == id) else { return; };
                                    saving.set(Some(id));
                                    message.set(format!("Saving House {id}…"));
                                    spawn(async move {
                                        match api::save(&current).await {
                                            Ok(saved) => {
                                                let mut all = properties();
                                                if let Some(existing) = all.iter_mut().find(|p| p.id == saved.id) { *existing = saved; }
                                                properties.set(all);
                                                message.set(format!("House {id} pricing saved."));
                                            }
                                            Err(error) => message.set(error),
                                        }
                                        saving.set(None);
                                    });
                                } }, if saving() == Some(property.id) { "Saving…" } else { "Save house" } }
                            }
                        }
                        if property.spaces.iter().all(|s| s.kind != "Bedroom" && s.kind != "Suite") { p { class: "empty", "No priced rooms yet." } }
                        for space in property.spaces.iter().filter(|s| s.kind == "Bedroom" || s.kind == "Suite") {
                            div { key: "{space.id}", class: "pricing-row",
                                div { class: "pricing-room", strong { "{space.name}" } small { if space.seasonal_prices_confirmed { "Confirmed" } else { "Review rates" } } }
                                label { "Current / mo", input { r#type: "number", min: "0", value: space.current_monthly_rent.map(|v| v.to_string()).unwrap_or_default(), oninput: { let pid = property.id; let sid = space.id; move |e| edit_rate(properties, pid, sid, "current", e.value()) } } }
                                label { "Sep–Dec", input { r#type: "number", min: "1", value: space.rent_sep_dec.map(|v| v.to_string()).unwrap_or_default(), oninput: { let pid = property.id; let sid = space.id; move |e| edit_rate(properties, pid, sid, "sep", e.value()) } } }
                                label { "Jan–Apr", input { r#type: "number", min: "1", value: space.rent_jan_apr.map(|v| v.to_string()).unwrap_or_default(), oninput: { let pid = property.id; let sid = space.id; move |e| edit_rate(properties, pid, sid, "jan", e.value()) } } }
                                label { "May–Aug", input { r#type: "number", min: "1", value: space.rent_may_aug.map(|v| v.to_string()).unwrap_or_default(), oninput: { let pid = property.id; let sid = space.id; move |e| edit_rate(properties, pid, sid, "may", e.value()) } } }
                                label { class: "check-label", input { r#type: "checkbox", checked: space.seasonal_prices_confirmed, onchange: { let pid = property.id; let sid = space.id; move |e| {
                                    let mut all = properties.write();
                                    if let Some(room) = all.iter_mut().find(|p| p.id == pid).and_then(|p| p.spaces.iter_mut().find(|s| s.id == sid)) { room.seasonal_prices_confirmed = e.checked(); }
                                } } } "Confirmed" }
                            }
                        }
                    }
                }
            }
        }
    }
}
