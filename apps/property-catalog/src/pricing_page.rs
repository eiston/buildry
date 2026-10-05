use crate::{
    api,
    house_badge::{house_color, HouseBadge},
    model::{price_for_month, starter_price_curve, Property, Space},
    CSS,
};
use dioxus::prelude::*;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const Y_MIN: u32 = 50;
const Y_MAX: u32 = 170;

#[derive(Clone, Copy, PartialEq)]
enum ActiveLine {
    Portfolio,
    House(u32),
    Room(u32, u32),
}

struct ChartLine {
    key: String,
    path: String,
    color: &'static str,
    class: &'static str,
}

fn room_values(room: &Space) -> [u32; 12] {
    let curve = room.effective_curve();
    std::array::from_fn(|index| price_for_month(100, &curve, (index + 1) as u8))
}

fn average_values(values: impl Iterator<Item = [u32; 12]>) -> [u32; 12] {
    let mut sums = [0_u64; 12];
    let mut count = 0_u64;
    for line in values {
        count += 1;
        for (index, value) in line.iter().enumerate() {
            sums[index] += *value as u64;
        }
    }
    if count == 0 {
        let reference = starter_price_curve();
        return std::array::from_fn(|index| price_for_month(100, &reference, (index + 1) as u8));
    }
    std::array::from_fn(|index| ((sums[index] + count / 2) / count) as u32)
}

fn house_values(house: &Property) -> [u32; 12] {
    average_values(
        house
            .spaces
            .iter()
            .filter(|room| room.kind == "Bedroom" || room.kind == "Suite")
            .map(room_values),
    )
}

fn portfolio_values(houses: &[Property]) -> [u32; 12] {
    average_values(houses.iter().map(house_values))
}

fn active_values(houses: &[Property], active: ActiveLine) -> [u32; 12] {
    match active {
        ActiveLine::Portfolio => portfolio_values(houses),
        ActiveLine::House(id) => houses
            .iter()
            .find(|house| house.id == id)
            .map(house_values)
            .unwrap_or_else(|| portfolio_values(houses)),
        ActiveLine::Room(house_id, room_id) => houses
            .iter()
            .find(|house| house.id == house_id)
            .and_then(|house| house.spaces.iter().find(|room| room.id == room_id))
            .map(room_values)
            .unwrap_or_else(|| portfolio_values(houses)),
    }
}

fn chart_x(month: u8) -> f64 {
    62.0 + (month as f64 - 1.0) * 812.0 / 11.0
}

fn chart_y(percent: u32) -> f64 {
    270.0 - (percent.clamp(Y_MIN, Y_MAX) - Y_MIN) as f64 * 240.0 / (Y_MAX - Y_MIN) as f64
}

fn chart_path(values: &[u32; 12]) -> String {
    let points: Vec<(f64, f64)> = (1..=12)
        .map(|month| (chart_x(month), chart_y(values[month as usize - 1])))
        .collect();
    let mut path = format!("M {:.1} {:.1}", points[0].0, points[0].1);
    for index in 0_usize..11 {
        let previous = points[index.saturating_sub(1)];
        let start = points[index];
        let end = points[index + 1];
        let next = points[(index + 2).min(11)];
        let control_1 = (
            start.0 + (end.0 - previous.0) / 6.0,
            start.1 + (end.1 - previous.1) / 6.0,
        );
        let control_2 = (
            end.0 - (next.0 - start.0) / 6.0,
            end.1 - (next.1 - start.1) / 6.0,
        );
        path.push_str(&format!(
            " C {:.1} {:.1}, {:.1} {:.1}, {:.1} {:.1}",
            control_1.0, control_1.1, control_2.0, control_2.1, end.0, end.1
        ));
    }
    path
}

fn chart_pointer(event: &PointerEvent) -> Option<(u8, u32)> {
    let chart = web_sys::window()?
        .document()?
        .query_selector("#pricing-curve-plot")
        .ok()??;
    let bounds = chart.get_bounding_client_rect();
    if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
        return None;
    }
    let position = event.client_coordinates();
    let x = (position.x - bounds.left()) * 900.0 / bounds.width();
    let y = (position.y - bounds.top()) * 330.0 / bounds.height();
    let month = (((x - 62.0) * 11.0 / 812.0).round() as i32 + 1).clamp(1, 12) as u8;
    let percent = (Y_MAX as f64 - (y - 30.0) * (Y_MAX - Y_MIN) as f64 / 240.0)
        .round()
        .clamp(Y_MIN as f64, Y_MAX as f64) as u32;
    Some((month, percent))
}

fn adjust_curve(
    mut properties: Signal<Vec<Property>>,
    mut dirty: Signal<Vec<u32>>,
    active: ActiveLine,
    month: u8,
    target: u32,
) {
    let current = active_values(&properties(), active)[month as usize - 1];
    let difference = target as i32 - current as i32;
    if difference == 0 {
        return;
    }
    let mut changed_houses = Vec::new();
    {
        let mut houses = properties.write();
        for house in houses.iter_mut() {
            if !matches!(active, ActiveLine::Portfolio)
                && !matches!(active, ActiveLine::House(id) if id == house.id)
                && !matches!(active, ActiveLine::Room(id, _) if id == house.id)
            {
                continue;
            }
            let mut changed = false;
            for room in house.spaces.iter_mut() {
                if room.kind != "Bedroom" && room.kind != "Suite" {
                    continue;
                }
                if matches!(active, ActiveLine::Room(_, room_id) if room.id != room_id) {
                    continue;
                }
                room.shift_price_month(month, difference);
                changed = true;
            }
            if changed {
                changed_houses.push(house.id);
            }
        }
    }
    for id in changed_houses {
        if !dirty().contains(&id) {
            dirty.write().push(id);
        }
    }
}

fn edit_base(
    mut properties: Signal<Vec<Property>>,
    mut dirty: Signal<Vec<u32>>,
    house_id: u32,
    room_id: u32,
    value: String,
) {
    let mut houses = properties.write();
    if let Some(room) = houses
        .iter_mut()
        .find(|house| house.id == house_id)
        .and_then(|house| house.spaces.iter_mut().find(|room| room.id == room_id))
    {
        room.current_monthly_rent = value.parse().ok();
        if !dirty().contains(&house_id) {
            dirty.write().push(house_id);
        }
    }
}

pub fn pricing_page() -> Element {
    let mut properties = use_signal(Vec::<Property>::new);
    let mut active = use_signal(|| ActiveLine::Portfolio);
    let mut active_month = use_signal(|| 9_u8);
    let mut dragging = use_signal(|| None::<u8>);
    let mut panning = use_signal(|| None::<(f64, f64)>);
    let mut zoom = use_signal(|| 100_u32);
    let mut config_open = use_signal(|| false);
    let mut show_market = use_signal(|| true);
    let mut visible_houses = use_signal(Vec::<u32>::new);
    let mut visible_rooms = use_signal(Vec::<(u32, u32)>::new);
    let mut search = use_signal(String::new);
    let mut dirty = use_signal(Vec::<u32>::new);
    let mut saving = use_signal(|| false);
    let mut loading = use_signal(|| true);
    let mut message = use_signal(String::new);
    use_effect(move || {
        spawn(async move {
            match api::list().await {
                Ok(data) => {
                    properties.set(data);
                }
                Err(error) => message.set(error),
            }
            loading.set(false);
        });
    });

    let all = properties();
    let query = search().to_lowercase();
    let current_active = active();
    let current_month = active_month();
    let current_values = active_values(&all, current_active);
    let current_percent = current_values[current_month as usize - 1];
    let room_count: usize = all
        .iter()
        .map(|house| {
            house
                .spaces
                .iter()
                .filter(|room| room.kind == "Bedroom" || room.kind == "Suite")
                .count()
        })
        .sum();
    let active_color = match current_active {
        ActiveLine::Portfolio => "#173F35",
        ActiveLine::House(id) | ActiveLine::Room(id, _) => house_color(id),
    };
    let chart_active_color = match current_active {
        ActiveLine::Portfolio => "#B2E99C",
        ActiveLine::House(id) | ActiveLine::Room(id, _) => house_color(id),
    };
    let active_label = match current_active {
        ActiveLine::Portfolio => "Portfolio".to_string(),
        ActiveLine::House(id) => format!("House {id}"),
        ActiveLine::Room(house_id, room_id) => all
            .iter()
            .find(|house| house.id == house_id)
            .and_then(|house| house.spaces.iter().find(|room| room.id == room_id))
            .map(|room| format!("House {house_id} · {}", room.name))
            .unwrap_or_else(|| format!("House {house_id}")),
    };
    let active_price = if let ActiveLine::Room(house_id, room_id) = current_active {
        all.iter()
            .find(|house| house.id == house_id)
            .and_then(|house| house.spaces.iter().find(|room| room.id == room_id))
            .and_then(|room| room.base_rent())
            .map(|base| base as u64 * current_percent as u64 / 100)
    } else {
        None
    };

    let mut lines = Vec::<ChartLine>::new();
    if show_market() {
        let reference = starter_price_curve();
        let values =
            std::array::from_fn(|index| price_for_month(100, &reference, (index + 1) as u8));
        lines.push(ChartLine {
            key: "market".into(),
            path: chart_path(&values),
            color: "#99A9A0",
            class: "market",
        });
    }
    for house in &all {
        if visible_houses().contains(&house.id) {
            lines.push(ChartLine {
                key: format!("house-{}", house.id),
                path: chart_path(&house_values(house)),
                color: house_color(house.id),
                class: if current_active == ActiveLine::House(house.id) {
                    "selected"
                } else {
                    "house"
                },
            });
        }
        for room in house
            .spaces
            .iter()
            .filter(|room| room.kind == "Bedroom" || room.kind == "Suite")
        {
            if visible_rooms().contains(&(house.id, room.id)) {
                lines.push(ChartLine {
                    key: format!("room-{}-{}", house.id, room.id),
                    path: chart_path(&room_values(room)),
                    color: house_color(house.id),
                    class: if current_active == ActiveLine::Room(house.id, room.id) {
                        "selected"
                    } else {
                        "room"
                    },
                });
            }
        }
    }
    lines.push(ChartLine {
        key: "portfolio".into(),
        path: chart_path(&portfolio_values(&all)),
        color: "#B2E99C",
        class: if current_active == ActiveLine::Portfolio {
            "selected"
        } else {
            "portfolio"
        },
    });
    lines.sort_by_key(|line| line.class == "selected");

    rsx! {
        document::Title { "Pricing | Buildry" }
        document::Stylesheet { href: CSS }
        div { class: "app-shell",
            aside { class: "sidebar",
                div { class: "sidebar-header", div { class: "brand", span { class: "brand-mark", "B" } span { class: "brand-name", "buildry" } } }
                a { class: "nav-item tour-nav-link", href: "/", title: "Properties", span { class: "nav-icon icon-properties" } span { class: "nav-text", "Properties" } }
                a { class: "nav-item tour-nav-link active", href: "/pricing", title: "Pricing", span { class: "nav-icon icon-pricing" } span { class: "nav-text", "Pricing" } }
                a { class: "nav-item tour-nav-link", href: "/tours", title: "House tours", span { class: "nav-icon icon-tours" } span { class: "nav-text", "House tours" } }
                a { class: "nav-item tour-nav-link", href: "/availability", title: "Availability", span { class: "nav-icon icon-availability" } span { class: "nav-text", "Availability" } }
                a { class: "nav-item tour-nav-link", href: "/stays", title: "Guest stays", span { class: "nav-icon icon-stays" } span { class: "nav-text", "Guest stays" } }
                form { class: "nav-logout", method: "post", action: "/logout", button { class: "nav-item tour-nav-link", r#type: "submit", title: "Sign out", span { class: "nav-icon icon-logout" } span { class: "nav-text", "Sign out" } } }
            }
            main { class: "main-content pricing-admin pricing-workspace",
                section { class: "page-head",
                    div { h1 { "Pricing" } p { class: "catalog-summary", "{all.len()} houses · {room_count} rooms and suites" } }
                    button { class: "primary-button", disabled: saving() || dirty().is_empty(), onclick: move |_| {
                        let updates: Vec<Property> = properties().into_iter().filter(|house| dirty().contains(&house.id)).collect();
                        saving.set(true); message.set(String::new());
                        spawn(async move {
                            for house in updates {
                                match api::save(&house).await {
                                    Ok(saved) => { let mut list = properties(); if let Some(item) = list.iter_mut().find(|item| item.id == saved.id) { *item = saved; } properties.set(list); dirty.write().retain(|id| *id != house.id); }
                                    Err(error) => { message.set(error); saving.set(false); return; }
                                }
                            }
                            message.set("Prices saved.".into()); saving.set(false);
                        });
                    }, if saving() { "Saving…" } else { "Save prices" } }
                }
                if loading() { p { class: "stays-state", "Loading prices…" } }
                if !message().is_empty() { p { class: "availability-message", role: "status", "{message}" } }
                div { class: "curve-workspace",
                    section { class: "curve-panel",
                        div { class: "curve-panel-head",
                            div { h2 { "Annual pricing" } p { "Drag points to adjust monthly prices. Drag the chart to pan." } }
                            div { class: "curve-legend",
                                label { input { r#type: "checkbox", checked: show_market(), onchange: move |event| show_market.set(event.checked()) } span { class: "legend-swatch market" } "Market reference" }
                                button { class: if current_active == ActiveLine::Portfolio { "active" } else { "" }, onclick: move |_| active.set(ActiveLine::Portfolio), span { class: "legend-swatch portfolio" } "Portfolio" }
                                div { class: "curve-zoom", title: "Zoom chart",
                                    button { aria_label: "Zoom out", disabled: zoom() <= 100, onclick: move |_| zoom.set(zoom().saturating_sub(25).max(100)), "−" }
                                    span { "{zoom()}%" }
                                    button { aria_label: "Zoom in", disabled: zoom() >= 300, onclick: move |_| zoom.set((zoom() + 25).min(300)), "+" }
                                }
                                button { class: if config_open() { "curve-config-button active" } else { "curve-config-button" }, aria_label: "Toggle houses and rooms panel", onclick: move |_| config_open.set(!config_open()), "Houses & rooms" }
                            }
                        }
                        div { class: "curve-chart-scroll", id: "pricing-curve-scroll",
                            svg { id: "pricing-curve-plot", class: "curve-chart", view_box: "0 0 900 330", preserve_aspect_ratio: "xMinYMin meet", role: "img", style: "width: {zoom()}%",
                                rect { x: "0", y: "0", width: "900", height: "330", fill: "#112B2B" }
                                for tick in [170_u32, 150, 130, 110, 90, 70, 50] {
                                    {
                                        let y = chart_y(tick);
                                        rsx! { line { class: "curve-grid-line", x1: "62", y1: "{y}", x2: "874", y2: "{y}" } text { class: "curve-axis-text", x: "51", y: "{y + 4.0}", text_anchor: "end", "{tick}%" } }
                                    }
                                }
                                text { class: "curve-axis-title", x: "17", y: "22", "% of base rent" }
                                for month in 1..=12_u8 { line { class: "curve-grid-vertical", x1: "{chart_x(month)}", y1: "30", x2: "{chart_x(month)}", y2: "270" } }
                                for (index, label) in MONTHS.iter().enumerate() { text { class: "curve-month-label", x: "{chart_x((index + 1) as u8)}", y: "307", text_anchor: "middle", "{label}" } }
                                path { class: "curve-area", d: "{chart_path(&portfolio_values(&all))} L 874 270 L 62 270 Z" }
                                line { class: "curve-crosshair", x1: "{chart_x(current_month)}", y1: "30", x2: "{chart_x(current_month)}", y2: "270" }
                                for line in lines { path { key: "{line.key}", class: "curve-line {line.class}", d: "{line.path}", stroke: "{line.color}", style: "color: {line.color}" } }
                                for month in 1..=12_u8 {
                                    circle { class: if active_month() == month { "curve-handle active" } else { "curve-handle" }, cx: "{chart_x(month)}", cy: "{chart_y(current_values[month as usize - 1])}", r: if active_month() == month { "7" } else { "5" }, fill: "{chart_active_color}" }
                                }
                                rect { class: "curve-drag-surface", x: "62", y: "30", width: "812", height: "240", fill: "transparent",
                                    onpointerdown: move |event| {
                                        if saving() { return; }
                                        event.prevent_default();
                                        if let Some((month, percent)) = chart_pointer(&event) {
                                            let current = active_values(&properties(), active())[month as usize - 1];
                                            if (percent as i32 - current as i32).abs() <= 12 {
                                                dragging.set(Some(month)); active_month.set(month);
                                                adjust_curve(properties, dirty, active(), month, percent);
                                            } else {
                                                let position = event.client_coordinates();
                                                panning.set(Some((position.x, position.y)));
                                            }
                                        }
                                    },
                                    onpointermove: move |event| {
                                        if let Some(month) = dragging() {
                                            if let Some((_, percent)) = chart_pointer(&event) { adjust_curve(properties, dirty, active(), month, percent); }
                                        } else if let Some((previous_x, previous_y)) = panning() {
                                            let position = event.client_coordinates();
                                            if let Some(scroller) = web_sys::window().and_then(|window| window.document()).and_then(|document| document.query_selector("#pricing-curve-scroll").ok().flatten()) {
                                                scroller.set_scroll_left(scroller.scroll_left() + (previous_x - position.x) as i32);
                                                scroller.set_scroll_top(scroller.scroll_top() + (previous_y - position.y) as i32);
                                            }
                                            panning.set(Some((position.x, position.y)));
                                        }
                                    },
                                    onpointerup: move |_| { dragging.set(None); panning.set(None); },
                                    onpointerleave: move |_| { dragging.set(None); panning.set(None); },
                                    onpointercancel: move |_| { dragging.set(None); panning.set(None); },
                                }
                            }
                        }
                        div { class: "curve-edit-bar", style: "--active-color: {active_color}",
                            div { strong { "{active_label}" } span { "{MONTHS[current_month as usize - 1]} · {current_percent}% of base" } if let Some(price) = active_price { b { "C${price} / month" } } }
                            div { class: "curve-nudge", button { aria_label: "Lower selected month by five percent", disabled: saving(), onclick: move |_| adjust_curve(properties, dirty, active(), active_month(), current_percent.saturating_sub(5).max(Y_MIN)), "−" } button { aria_label: "Raise selected month by five percent", disabled: saving(), onclick: move |_| adjust_curve(properties, dirty, active(), active_month(), (current_percent + 5).min(Y_MAX)), "+" } }
                        }
                        div { class: "curve-month-buttons", for (index, label) in MONTHS.iter().enumerate() { button { class: if current_month == (index + 1) as u8 { "active" } else { "" }, onclick: move |_| active_month.set((index + 1) as u8), "{label}" } } }
                    }
                    if config_open() { section { class: "curve-house-panel",
                        input { class: "search-input", aria_label: "Search houses and rooms", placeholder: "Search house or room", value: "{search}", oninput: move |event| search.set(event.value()) }
                        for house in all.iter().filter(|house| query.is_empty() || house.id.to_string().contains(&query) || house.name.to_lowercase().contains(&query) || house.address.to_lowercase().contains(&query) || house.spaces.iter().any(|room| room.name.to_lowercase().contains(&query))) {
                            div { key: "{house.id}", class: "curve-house-group", style: "--house-color: {house_color(house.id)}",
                                div { class: "curve-house-head",
                                    HouseBadge { id: house.id }
                                    button { class: if current_active == ActiveLine::House(house.id) { "curve-select active" } else { "curve-select" }, onclick: { let house_id = house.id; move |_| { active.set(ActiveLine::House(house_id)); if !visible_houses().contains(&house_id) { visible_houses.write().push(house_id); } } }, "{house.address}" }
                                    label { class: "curve-toggle", title: "Show house line", input { r#type: "checkbox", aria_label: "Show line for house {house.id}", checked: visible_houses().contains(&house.id), onchange: { let house_id = house.id; move |event| { if event.checked() { if !visible_houses().contains(&house_id) { visible_houses.write().push(house_id); } active.set(ActiveLine::House(house_id)); } else { visible_houses.write().retain(|id| *id != house_id); if active() == ActiveLine::House(house_id) { active.set(ActiveLine::Portfolio); } } } } } }
                                }
                                for room in house.spaces.iter().filter(|room| (room.kind == "Bedroom" || room.kind == "Suite") && (query.is_empty() || house.id.to_string().contains(&query) || house.name.to_lowercase().contains(&query) || house.address.to_lowercase().contains(&query) || room.name.to_lowercase().contains(&query))) {
                                    div { key: "{room.id}", class: "curve-room-row",
                                        label { class: "curve-toggle", title: "Show room line", input { r#type: "checkbox", aria_label: "Show line for {room.name} in house {house.id}", checked: visible_rooms().contains(&(house.id, room.id)), onchange: { let house_id = house.id; let room_id = room.id; move |event| { if event.checked() { if !visible_rooms().contains(&(house_id, room_id)) { visible_rooms.write().push((house_id, room_id)); } active.set(ActiveLine::Room(house_id, room_id)); } else { visible_rooms.write().retain(|item| *item != (house_id, room_id)); if active() == ActiveLine::Room(house_id, room_id) { active.set(ActiveLine::Portfolio); } } } } } }
                                        button { class: if current_active == ActiveLine::Room(house.id, room.id) { "curve-select active" } else { "curve-select" }, onclick: { let house_id = house.id; let room_id = room.id; move |_| { active.set(ActiveLine::Room(house_id, room_id)); if !visible_rooms().contains(&(house_id, room_id)) { visible_rooms.write().push((house_id, room_id)); } } }, "{room.name}" }
                                        label { class: "curve-base-field", span { "$" } input { r#type: "number", min: "1", aria_label: "Base monthly rent for {room.name} in house {house.id}", value: room.base_rent().map(|value| value.to_string()).unwrap_or_default(), oninput: { let house_id = house.id; let room_id = room.id; move |event| edit_base(properties, dirty, house_id, room_id, event.value()) } } }
                                    }
                                }
                            }
                        }
                    } }
                }
            }
        }
    }
}
