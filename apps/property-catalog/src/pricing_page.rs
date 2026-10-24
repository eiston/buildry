use crate::icon::Icon;
use crate::{
    api,
    model::{MonthlyPriceOverride, Property, Space},
    CSS,
};
use dioxus::prelude::*;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const VISIBLE_MONTHS: usize = 18;

fn month_at(index: i32) -> (i32, u8) {
    (index.div_euclid(12), index.rem_euclid(12) as u8 + 1)
}

fn month_label(index: i32) -> String {
    let (year, month) = month_at(index);
    format!("{} {year}", MONTHS[month as usize - 1])
}

fn inherited_price(room: &Space, month: u8) -> Option<u32> {
    room.base_rent()
        .map(|base| crate::model::price_for_month(base, &room.effective_curve(), month))
}

fn set_monthly_price(room: &mut Space, start: i32, end: i32, price: Option<u32>) {
    room.monthly_price_overrides.retain(|item| {
        let index = item.year * 12 + item.month as i32 - 1;
        index < start || index > end
    });
    if let Some(price) = price {
        for index in start..=end {
            let (year, month) = month_at(index);
            room.monthly_price_overrides
                .push(MonthlyPriceOverride { year, month, price });
        }
        room.monthly_price_overrides
            .sort_by_key(|item| (item.year, item.month));
    }
}

pub(crate) fn current_room_price(room: &Space) -> Option<u32> {
    let today = js_sys::Date::new_0();
    room.price_for_calendar_month(today.get_full_year() as i32, today.get_month() as u8 + 1)
}

#[derive(Clone, Copy, PartialEq)]
struct Selection {
    house_id: u32,
    room_id: u32,
    start: i32,
}

pub fn pricing_page() -> Element {
    let today = js_sys::Date::new_0();
    let current_month = today.get_full_year() as i32 * 12 + today.get_month() as i32;
    let mut first_month = use_signal(|| current_month - 2);
    let mut properties = use_signal(Vec::<Property>::new);
    let mut dirty = use_signal(Vec::<u32>::new);
    let mut selection = use_signal(|| None::<Selection>);
    let mut end_month = use_signal(|| current_month);
    let mut price_draft = use_signal(String::new);
    let mut loading = use_signal(|| true);
    let mut saving = use_signal(|| false);
    let mut message = use_signal(String::new);

    use_effect(move || {
        spawn(async move {
            match api::list().await {
                Ok(data) => properties.set(data),
                Err(error) => message.set(error),
            }
            loading.set(false);
        });
    });

    let houses = properties();
    let months: Vec<i32> = (0..VISIBLE_MONTHS)
        .map(|offset| first_month() + offset as i32)
        .collect();
    let grid_style = format!(
        "grid-template-columns: minmax(245px, 245px) repeat({}, minmax(108px, 108px));",
        months.len()
    );
    let room_count = houses
        .iter()
        .flat_map(|house| house.spaces.iter())
        .filter(|room| room.kind == "Bedroom" || room.kind == "Suite")
        .count();
    let selected_room = selection().and_then(|selected| {
        houses
            .iter()
            .find(|house| house.id == selected.house_id)
            .and_then(|house| house.spaces.iter().find(|room| room.id == selected.room_id))
            .map(|room| (selected, room.clone()))
    });

    rsx! {
        document::Title { "Pricing | Buildry" }
        document::Stylesheet { href: CSS }
        crate::admin_shell::AdminShell {
            active: "/pricing",
            property_count: None,
            main_class: "main-content pricing-calendar-page",
            section { class: "page-head",
                div {
                    h1 { "Pricing" }
                    p { class: "catalog-summary", "{houses.len()} houses · {room_count} rooms and suites" }
                }
                div { class: "pricing-head-actions",
                    if !dirty().is_empty() { span { class: "pricing-unsaved", "Unsaved changes" } }
                    button {
                        class: "primary-button",
                        disabled: saving() || dirty().is_empty(),
                        onclick: move |_| {
                            let updates: Vec<Property> = properties().into_iter()
                                .filter(|house| dirty().contains(&house.id)).collect();
                            saving.set(true);
                            message.set(String::new());
                            spawn(async move {
                                for house in updates {
                                    match api::save(&house).await {
                                        Ok(saved) => {
                                            let mut list = properties();
                                            if let Some(item) = list.iter_mut().find(|item| item.id == saved.id) {
                                                *item = saved;
                                            }
                                            properties.set(list);
                                            dirty.write().retain(|id| *id != house.id);
                                        }
                                        Err(error) => {
                                            message.set(error);
                                            saving.set(false);
                                            return;
                                        }
                                    }
                                }
                                message.set("Prices saved.".into());
                                saving.set(false);
                            });
                        },
                        if saving() { "Saving…" } else { "Save prices" }
                    }
                }
            }
            div { class: "pricing-calendar-toolbar",
                p { "Set a base monthly rent for each room. Select a month to set a price for that month or a range." }
                div { class: "pricing-calendar-nav",
                    button { aria_label: "Previous six months", onclick: move |_| first_month.set(first_month() - 6), Icon { name: "chevron-left" } }
                    button { onclick: move |_| first_month.set(current_month - 2), "Today" }
                    button { aria_label: "Next six months", onclick: move |_| first_month.set(first_month() + 6), Icon { name: "chevron-right" } }
                }
            }
            if loading() { p { class: "stays-state", "Loading prices…" } }
            if !message().is_empty() { p { class: "availability-message", role: "status", "{message}" } }
            if houses.is_empty() && !loading() { p { class: "stays-state", "No houses are available yet." } }
            if !houses.is_empty() {
                div { class: "pricing-matrix-scroll", tabindex: "0", aria_label: "Monthly room prices; scroll horizontally for more months",
                    div { class: "pricing-matrix",
                        div { class: "pricing-matrix-header", style: "{grid_style}",
                            span { "Room / base rent" }
                            for index in &months {
                                {
                                    let (year, month) = month_at(*index);
                                    rsx! { span { key: "{index}", class: if *index == current_month { "current" } else { "" }, "{MONTHS[month as usize - 1]}" small { "{year}" } } }
                                }
                            }
                        }
                        for house in &houses {
                            div { key: "{house.id}", class: "pricing-house-row", style: "{grid_style}",
                                div {
                                    strong { "{house.name}" }
                                    if house.name != house.address { small { "{house.address}" } }
                                }
                            }
                            for room in house.spaces.iter().filter(|room| room.kind == "Bedroom" || room.kind == "Suite") {
                                {
                                    let house_id = house.id;
                                    let room_id = room.id;
                                    let base = room.base_rent();
                                    rsx! {
                                        div { key: "{house_id}-{room_id}", class: "pricing-room-row", style: "{grid_style}",
                                            div { class: "pricing-room-name",
                                                strong { "{room.name}" }
                                                label {
                                                    span { "Base C$ / month" }
                                                    crate::form_input::FormInput {
                                                        r#type: "number",
                                                        min: "1",
                                                        max: "100000",
                                                        aria_label: "Base monthly rent for {room.name} in {house.name}",
                                                        value: "{base.map(|price| price.to_string()).unwrap_or_default()}",
                                                        placeholder: "Set price",
                                                        onchange: move |event: FormEvent| {
                                                            let value = event.value();
                                                            let parsed = value.parse::<u32>().ok().filter(|price| *price > 0 && *price <= 100_000);
                                                            let Some(parsed) = parsed else {
                                                                message.set("Enter a base monthly rent between C$1 and C$100,000.".into());
                                                                return;
                                                            };
                                                            let mut list = properties.write();
                                                            if let Some(target) = list.iter_mut().find(|house| house.id == house_id)
                                                                .and_then(|house| house.spaces.iter_mut().find(|room| room.id == room_id)) {
                                                                target.current_monthly_rent = Some(parsed);
                                                                target.rent_is_estimate = false;
                                                                if !dirty().contains(&house_id) { dirty.write().push(house_id); }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            for index in &months {
                                                {
                                                    let (year, month) = month_at(*index);
                                                    let price = room.price_for_calendar_month(year, month);
                                                    let overridden = room.monthly_price_overrides.iter().any(|item| item.year == year && item.month == month);
                                                    let seasonal = !overridden && price.is_some() && price != base;
                                                    let cell_class = if overridden { "pricing-month-price overridden" } else if seasonal { "pricing-month-price seasonal" } else { "pricing-month-price" };
                                                    let month_index = *index;
                                                    rsx! {
                                                        button {
                                                            key: "{index}",
                                                            class: "{cell_class}",
                                                            disabled: saving(),
                                                            aria_label: "Edit {room.name} price for {MONTHS[month as usize - 1]} {year}",
                                                            onclick: move |_| {
                                                                selection.set(Some(Selection { house_id, room_id, start: month_index }));
                                                                end_month.set(month_index);
                                                                price_draft.set(price.map(|amount| amount.to_string()).unwrap_or_default());
                                                            },
                                                            strong { if let Some(price) = price { "C${price}" } else { "Set base" } }
                                                            if overridden { small { "Custom" } }
                                                            else if seasonal { small { "Seasonal" } }
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
                }
            }
            if let Some((selected, room)) = selected_room {
                {
                    let (year, month) = month_at(selected.start);
                    let existing = room.monthly_price_overrides.iter().any(|item| item.year == year && item.month == month);
                    let inherited = inherited_price(&room, month);
                    let can_apply = price_draft().parse::<u32>().ok().is_some_and(|price| (1..=100_000).contains(&price))
                        && room.base_rent().is_some()
                        && end_month() >= selected.start && end_month() - selected.start < 24;
                    let range_months = end_month() - selected.start + 1;
                    let range_label = if range_months == 1 { "month" } else { "months" };
                    let current_total = (selected.start..=end_month()).try_fold(0_u64, |total, index| {
                        let (year, month) = month_at(index);
                        room.price_for_calendar_month(year, month).map(|price| total + price as u64)
                    });
                    let proposed_total = price_draft().parse::<u32>().ok()
                        .filter(|price| (1..=100_000).contains(price))
                        .map(|price| price as u64 * range_months as u64);
                    rsx! {
                        div { class: "pricing-editor-backdrop", onclick: move |_| selection.set(None),
                            section { class: "pricing-editor", role: "dialog", aria_modal: "true", aria_label: "Edit monthly price",
                                onclick: move |event| event.stop_propagation(),
                                div { class: "pricing-editor-head",
                                    div { small { "PRICE OVERRIDE" } h2 { "{room.name}" } p { "{month_label(selected.start)} · {houses.iter().find(|house| house.id == selected.house_id).map(|house| house.name.as_str()).unwrap_or_default()}" } }
                                    button { aria_label: "Close price editor", onclick: move |_| selection.set(None), Icon { name: "close" } }
                                }
                                crate::app_form::AppForm { class: "pricing-editor-form", onsubmit: move |event: FormEvent| {
                                    event.prevent_default();
                                    if !can_apply { return; }
                                    if let Ok(price) = price_draft().parse::<u32>() {
                                        let mut list = properties.write();
                                        if let Some(target) = list.iter_mut().find(|house| house.id == selected.house_id)
                                            .and_then(|house| house.spaces.iter_mut().find(|room| room.id == selected.room_id)) {
                                            set_monthly_price(target, selected.start, end_month(), Some(price));
                                            if !dirty().contains(&selected.house_id) { dirty.write().push(selected.house_id); }
                                        }
                                        selection.set(None);
                                    }
                                },
                                div { class: "pricing-editor-body",
                                    p { "Set one monthly price for the selected month or a consecutive range. It applies only to these calendar months." }
                                    crate::app_form::FormRows { class: Some("pricing-editor-fields".to_string()),
                                        crate::form_select::FormField { label: "From", required: false, strong { "{month_label(selected.start)}" } }
                                        crate::form_select::FormSelect { label: "Through", required: true, value: end_month().to_string(), placeholder: "Choose month", options: (0..24_i32).map(|offset|{let index=selected.start+offset;(index.to_string(),month_label(index))}).collect::<Vec<_>>(), onchange: move |value:String| {if let Ok(value)=value.parse::<i32>(){end_month.set(value)}} }
                                    }
                                    crate::app_form::FormRows { class: Some("pricing-editor-price".to_string()),
                                        crate::form_select::FormField { label: "Monthly price (C$)", required: true,
                                            crate::form_input::FormInput { r#type: "number", min: "1", max: "100000", required: true,
                                                aria_label: "Monthly override price", value: "{price_draft}", oninput: move |event: FormEvent| price_draft.set(event.value()),
                                            }
                                        }
                                    }
                                    if range_months == 1 {
                                        if let Some(inherited) = inherited {
                                        p { class: "pricing-editor-inherited", "Without an override: C${inherited} / month" }
                                        }
                                    }
                                    if room.base_rent().is_none() {
                                        p { class: "pricing-editor-inherited", "Set this room's base monthly rent first." }
                                    }
                                    if let Some(proposed_total) = proposed_total {
                                        div { class: "pricing-editor-preview",
                                            span { "Total for {range_months} {range_label}" }
                                            strong { "C${proposed_total}" }
                                            if let Some(current_total) = current_total {
                                                small { "Currently C${current_total} for the same period" }
                                            }
                                        }
                                    }
                                }
                                div { class: "pricing-editor-actions",
                                    if existing {
                                        button { class: "pricing-reset", r#type: "button", onclick: move |_| {
                                            let mut list = properties.write();
                                            if let Some(target) = list.iter_mut().find(|house| house.id == selected.house_id)
                                                .and_then(|house| house.spaces.iter_mut().find(|room| room.id == selected.room_id)) {
                                                set_monthly_price(target, selected.start, end_month(), None);
                                                if !dirty().contains(&selected.house_id) { dirty.write().push(selected.house_id); }
                                            }
                                            selection.set(None);
                                        }, "Remove override" }
                                    }
                                    button { class: "primary-button", r#type: "submit", disabled: !can_apply, "Apply price" }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_are_exact_calendar_months() {
        let mut room = Space::new(1);
        room.current_monthly_rent = Some(1000);
        set_monthly_price(&mut room, 2026 * 12 + 11, 2027 * 12, Some(1200));
        assert_eq!(room.price_for_calendar_month(2026, 12), Some(1200));
        assert_eq!(room.price_for_calendar_month(2027, 1), Some(1200));
        assert_ne!(room.price_for_calendar_month(2027, 12), Some(1200));
        set_monthly_price(&mut room, 2026 * 12 + 11, 2027 * 12, None);
        assert!(room.monthly_price_overrides.is_empty());
    }
}
