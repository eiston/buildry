use crate::{
    api,
    house_badge::{house_color, HouseBadge},
    model::{percent_for_month, price_for_month, starter_price_curve, PricePoint, Property, Space},
    CSS,
};
use dioxus::prelude::*;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const Y_MIN: u32 = 50;
const Y_MAX: u32 = 170;
const CHART_TOP: f64 = 0.0;
const CHART_BOTTOM: f64 = 1000.0;
const CHART_HEIGHT: f64 = 1000.0;
const CHART_WIDTH: f64 = 900.0;

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
    (month as f64 - 0.5) * CHART_WIDTH / 12.0
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 31,
    }
}

fn day_of_year(year: i32, month: u8, day: u8) -> u16 {
    (1..month)
        .map(|prior| days_in_month(year, prior) as u16)
        .sum::<u16>()
        + day as u16
}

fn date_from_day(year: i32, ordinal: u16) -> (u8, u8) {
    let mut remaining = ordinal.max(1);
    for month in 1..=12 {
        let days = days_in_month(year, month) as u16;
        if remaining <= days {
            return (month, remaining as u8);
        }
        remaining -= days;
    }
    (12, 31)
}

fn chart_x_for_day(year: i32, ordinal: u16) -> f64 {
    let (month, day) = date_from_day(year, ordinal);
    ((month as f64 - 1.0) + (day as f64 - 0.5) / days_in_month(year, month) as f64) * CHART_WIDTH
        / 12.0
}

fn day_from_chart_x(year: i32, x: f64) -> u16 {
    let position = (x * 12.0 / CHART_WIDTH).clamp(0.0, 12.0 - f64::EPSILON);
    let month = position.floor() as u8 + 1;
    let day = ((position.fract() * days_in_month(year, month) as f64).floor() as u8 + 1)
        .min(days_in_month(year, month));
    day_of_year(year, month, day)
}

fn month_span_for_day(year: i32, ordinal: u16) -> (u8, u8, f64) {
    let x = chart_x_for_day(year, ordinal);
    if x <= chart_x(1) {
        return (1, 1, 0.0);
    }
    if x >= chart_x(12) {
        return (12, 12, 0.0);
    }
    let position = (x - chart_x(1)) * 12.0 / CHART_WIDTH;
    let left = position.floor() as u8 + 1;
    (left, left + 1, position.fract())
}

fn percent_for_day(points: &[PricePoint], year: i32, ordinal: u16) -> f64 {
    let (left, right, weight) = month_span_for_day(year, ordinal);
    let a = percent_for_month(points, left);
    let b = percent_for_month(points, right);
    a * (1.0 - weight) + b * weight
}

fn chart_y(percent: u32) -> f64 {
    CHART_BOTTOM
        - (percent.clamp(Y_MIN, Y_MAX) - Y_MIN) as f64 * (CHART_BOTTOM - CHART_TOP)
            / (Y_MAX - Y_MIN) as f64
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

fn chart_coordinates(event: &PointerEvent) -> Option<(f64, f64)> {
    let chart = web_sys::window()?
        .document()?
        .query_selector("#pricing-curve-plot")
        .ok()??;
    let bounds = chart.get_bounding_client_rect();
    if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
        return None;
    }
    let position = event.client_coordinates();
    let scale = (bounds.width() / CHART_WIDTH).max(bounds.height() / CHART_HEIGHT);
    let crop_x = (CHART_WIDTH * scale - bounds.width()) / 2.0;
    let crop_y = (CHART_HEIGHT * scale - bounds.height()) / 2.0;
    let x = (position.x - bounds.left() + crop_x) / scale;
    let y = (position.y - bounds.top() + crop_y) / scale;
    Some((x, y))
}

fn chart_pointer(event: &PointerEvent) -> Option<(u8, u32)> {
    let (x, y) = chart_coordinates(event)?;
    let month = (((x - chart_x(1)) * 12.0 / CHART_WIDTH).round() as i32 + 1).clamp(1, 12) as u8;
    let percent = (Y_MAX as f64
        - (y - CHART_TOP) * (Y_MAX - Y_MIN) as f64 / (CHART_BOTTOM - CHART_TOP))
        .round()
        .clamp(Y_MIN as f64, Y_MAX as f64) as u32;
    Some((month, percent))
}

fn curve_y_at_x(values: &[u32; 12], x: f64) -> f64 {
    let position = ((x - chart_x(1)) * 12.0 / CHART_WIDTH).clamp(0.0, 11.0);
    let index = (position.floor() as usize).min(10);
    let t = position - index as f64;
    let y0 = chart_y(values[index.saturating_sub(1)]);
    let y1 = chart_y(values[index]);
    let y2 = chart_y(values[index + 1]);
    let y3 = chart_y(values[(index + 2).min(11)]);
    let control_1 = y1 + (y2 - y0) / 6.0;
    let control_2 = y2 - (y3 - y1) / 6.0;
    let inverse = 1.0 - t;
    inverse.powi(3) * y1
        + 3.0 * inverse.powi(2) * t * control_1
        + 3.0 * inverse * t.powi(2) * control_2
        + t.powi(3) * y2
}

fn nearby_lines(
    houses: &[Property],
    shown_houses: &[u32],
    shown_rooms: &[(u32, u32)],
    x: f64,
    y: f64,
) -> Vec<ActiveLine> {
    if !(chart_x(1)..=chart_x(12)).contains(&x) {
        return Vec::new();
    }
    let mut matches = Vec::new();
    let portfolio = portfolio_values(houses);
    matches.push((
        ActiveLine::Portfolio,
        (curve_y_at_x(&portfolio, x) - y).abs(),
    ));
    for house in houses {
        if shown_houses.contains(&house.id) {
            matches.push((
                ActiveLine::House(house.id),
                (curve_y_at_x(&house_values(house), x) - y).abs(),
            ));
        }
        for room in house.spaces.iter().filter(|room| {
            (room.kind == "Bedroom" || room.kind == "Suite")
                && shown_rooms.contains(&(house.id, room.id))
        }) {
            matches.push((
                ActiveLine::Room(house.id, room.id),
                (curve_y_at_x(&room_values(room), x) - y).abs(),
            ));
        }
    }
    matches.retain(|(_, distance)| *distance <= 8.0);
    matches.sort_by(|a, b| a.1.total_cmp(&b.1));
    matches.into_iter().map(|(line, _)| line).collect()
}

fn line_label(houses: &[Property], line: ActiveLine) -> String {
    match line {
        ActiveLine::Portfolio => "Portfolio".to_string(),
        ActiveLine::House(id) => houses
            .iter()
            .find(|house| house.id == id)
            .map(|house| format!("House {id} · {}", house.address))
            .unwrap_or_else(|| format!("House {id}")),
        ActiveLine::Room(house_id, room_id) => houses
            .iter()
            .find(|house| house.id == house_id)
            .and_then(|house| {
                house
                    .spaces
                    .iter()
                    .find(|room| room.id == room_id)
                    .map(|room| format!("House {house_id} · {} · {}", room.name, house.address))
            })
            .unwrap_or_else(|| format!("House {house_id} · Room {room_id}")),
    }
}

fn room_price_at(room: &Space, year: i32, day: u16) -> Option<u32> {
    room.base_rent().map(|base| {
        (base as f64 * percent_for_day(&room.effective_curve(), year, day) / 100.0).round() as u32
    })
}

fn house_price_at(house: &Property, year: i32, day: u16) -> u32 {
    house
        .spaces
        .iter()
        .filter(|room| room.kind == "Bedroom" || room.kind == "Suite")
        .filter_map(|room| room_price_at(room, year, day))
        .sum()
}

fn update_room_price_at_day(room: &mut Space, year: i32, day: u16, dollars: u32) -> bool {
    let Some(base) = room.base_rent().filter(|base| *base > 0) else {
        return false;
    };
    let curve = room.effective_curve();
    let current = percent_for_day(&curve, year, day);
    let requested = dollars as f64 * 100.0 / base as f64;
    let difference = requested - current;
    let (left, right, _) = month_span_for_day(year, day);
    room.price_curve = (1..=12)
        .map(|month| PricePoint {
            month,
            percent: (percent_for_month(&curve, month)
                + if month == left || month == right {
                    difference
                } else {
                    0.0
                })
            .clamp(1.0, 500.0),
        })
        .collect();
    true
}

fn edit_day_price(
    mut properties: Signal<Vec<Property>>,
    mut dirty: Signal<Vec<u32>>,
    house_id: u32,
    room_id: u32,
    year: i32,
    day: u16,
    value: String,
) {
    let Ok(dollars) = value.parse::<u32>() else {
        return;
    };
    let mut houses = properties.write();
    if let Some(room) = houses
        .iter_mut()
        .find(|house| house.id == house_id)
        .and_then(|house| house.spaces.iter_mut().find(|room| room.id == room_id))
    {
        if update_room_price_at_day(room, year, day, dollars) && !dirty().contains(&house_id) {
            dirty.write().push(house_id);
        }
    }
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
        update_room_base(room, &value);
        if !dirty().contains(&house_id) {
            dirty.write().push(house_id);
        }
    }
}

fn update_room_base(room: &mut Space, value: &str) {
    let curve = room.effective_curve();
    room.price_curve = curve;
    room.current_monthly_rent = value.parse().ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_base_keeps_month_percentages_and_updates_quotes() {
        let mut room = Space::new(1);
        room.current_monthly_rent = Some(950);
        room.rent_jan_apr = Some(950);
        room.rent_may_aug = Some(950);
        room.rent_sep_dec = Some(1007);
        let before = price_for_month(100, &room.effective_curve(), 9);
        update_room_base(&mut room, "1000");
        assert_eq!(room.base_rent(), Some(1000));
        assert_eq!(price_for_month(100, &room.effective_curve(), 9), before);
        assert_eq!(
            room_price_at(&room, 2026, day_of_year(2026, 9, 15)),
            Some(before * 10)
        );
    }

    #[test]
    fn editing_daily_price_sets_exact_rent() {
        let mut room = Space::new(1);
        room.current_monthly_rent = Some(950);
        room.price_curve = (1..=12)
            .map(|month| PricePoint {
                month,
                percent: 100.0,
            })
            .collect();
        let day = day_of_year(2026, 10, 5);
        assert!(update_room_price_at_day(&mut room, 2026, day, 1007));
        assert_eq!(room_price_at(&room, 2026, day), Some(1007));
        assert_eq!(day_from_chart_x(2026, chart_x_for_day(2026, day)), day);
    }
}

pub fn pricing_page() -> Element {
    let mut properties = use_signal(Vec::<Property>::new);
    let mut active = use_signal(|| ActiveLine::Portfolio);
    let mut active_month = use_signal(|| 9_u8);
    let today = js_sys::Date::new_0();
    let chart_year = today.get_full_year() as i32;
    let today_day = day_of_year(
        chart_year,
        today.get_month() as u8 + 1,
        today.get_date() as u8,
    );
    let mut cursor_day = use_signal(|| today_day);
    let mut cursor_dragging = use_signal(|| false);
    let mut dragging = use_signal(|| None::<u8>);
    let mut press_hits = use_signal(|| None::<(u8, Vec<ActiveLine>, f64, f64)>);
    let mut press_moved = use_signal(|| false);
    let mut line_picker = use_signal(|| None::<(u8, Vec<ActiveLine>)>);
    let mut panning = use_signal(|| None::<(f64, f64)>);
    let mut zoom = use_signal(|| 100_u32);
    let mut config_open = use_signal(|| true);
    let mut show_market = use_signal(|| true);
    let mut visible_houses = use_signal(Vec::<u32>::new);
    let mut visible_rooms = use_signal(Vec::<(u32, u32)>::new);
    let mut search = use_signal(String::new);
    let mut dirty = use_signal(Vec::<u32>::new);
    let mut price_draft = use_signal(|| None::<(u32, u32, u16, String)>);
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
    let viewed_day = cursor_day();
    let (viewed_month, viewed_date) = date_from_day(chart_year, viewed_day);
    let viewed_label = format!("{} {}", MONTHS[viewed_month as usize - 1], viewed_date);
    let today_label = format!(
        "{} {}",
        MONTHS[today.get_month() as usize],
        today.get_date()
    );
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
                div { class: if config_open() { "curve-workspace" } else { "curve-workspace collapsed" },
                    if !config_open() { button { class: "curve-side-rail", aria_label: "Show houses and rooms", onclick: move |_| config_open.set(true), span { "›" } span { "Houses & rooms" } } }
                    section { class: "curve-panel",
                        div { class: "curve-panel-head",
                            div { h2 { "Annual pricing" } p { "{active_label} · {MONTHS[current_month as usize - 1]} {current_percent}%" } }
                            div { class: "curve-legend",
                                label { input { r#type: "checkbox", checked: show_market(), onchange: move |event| show_market.set(event.checked()) } span { class: "legend-swatch market" } "Market reference" }
                                button { class: if current_active == ActiveLine::Portfolio { "active" } else { "" }, onclick: move |_| { active.set(ActiveLine::Portfolio); line_picker.set(None); }, span { class: "legend-swatch portfolio" } "Portfolio" }
                                div { class: "curve-nudge", button { aria_label: "Lower selected month by five percent", disabled: saving(), onclick: move |_| adjust_curve(properties, dirty, active(), active_month(), current_percent.saturating_sub(5).max(Y_MIN)), "−" } button { aria_label: "Raise selected month by five percent", disabled: saving(), onclick: move |_| adjust_curve(properties, dirty, active(), active_month(), (current_percent + 5).min(Y_MAX)), "+" } }
                                div { class: "curve-zoom", title: "Zoom chart",
                                    button { aria_label: "Zoom out", disabled: zoom() <= 100, onclick: move |_| zoom.set(zoom().saturating_sub(25).max(100)), "−" }
                                    span { "{zoom()}%" }
                                    button { aria_label: "Zoom in", disabled: zoom() >= 300, onclick: move |_| zoom.set((zoom() + 25).min(300)), "+" }
                                }
                            }
                        }
                        div { class: "curve-chart-scroll", id: "pricing-curve-scroll",
                            svg { id: "pricing-curve-plot", class: "curve-chart", view_box: "0 0 900 1000", preserve_aspect_ratio: "xMidYMid slice", role: "img", style: "width: {zoom()}%; height: {zoom()}%",
                                rect { x: "0", y: "0", width: "900", height: "1000", fill: "#112B2B" }
                                for tick in [170_u32, 150, 130, 110, 90, 70, 50] {
                                    {
                                        let y = chart_y(tick);
                                        rsx! { line { class: "curve-grid-line", x1: "0", y1: "{y}", x2: "900", y2: "{y}" } }
                                    }
                                }
                                line { class: "curve-baseline", x1: "0", y1: "{chart_y(100)}", x2: "900", y2: "{chart_y(100)}" }
                                text { class: "curve-baseline-label", x: "26", y: "{chart_y(100) - 7.0}", "100% base" }
                                for boundary in 0..=12_u8 { line { class: "curve-grid-vertical", x1: "{boundary as f64 * CHART_WIDTH / 12.0}", y1: "0", x2: "{boundary as f64 * CHART_WIDTH / 12.0}", y2: "1000" } }
                                path { class: "curve-area", d: "{chart_path(&portfolio_values(&all))} L 900 1000 L 0 1000 Z" }
                                line { class: "curve-today-line", x1: "{chart_x_for_day(chart_year, today_day)}", y1: "0", x2: "{chart_x_for_day(chart_year, today_day)}", y2: "1000" }
                                text { class: "curve-today-text", x: "{chart_x_for_day(chart_year, today_day) + 5.0}", y: "55", "Today · {today_label}" }
                                line { class: "curve-crosshair", x1: "{chart_x(current_month)}", y1: "0", x2: "{chart_x(current_month)}", y2: "1000" }
                                line { class: "curve-cursor-line", x1: "{chart_x_for_day(chart_year, viewed_day)}", y1: "0", x2: "{chart_x_for_day(chart_year, viewed_day)}", y2: "1000" }
                                rect { class: "curve-cursor-label", x: "{(chart_x_for_day(chart_year, viewed_day) - 31.0).clamp(0.0, 838.0)}", y: "12", width: "62", height: "27", rx: "8" }
                                text { class: "curve-cursor-text", x: "{(chart_x_for_day(chart_year, viewed_day)).clamp(31.0, 869.0)}", y: "30", text_anchor: "middle", "{viewed_label}" }
                                for line in lines { path { key: "{line.key}", class: "curve-line {line.class}", d: "{line.path}", stroke: "{line.color}", style: "color: {line.color}" } }
                                for month in 1..=12_u8 {
                                    circle { class: if active_month() == month { "curve-handle active" } else { "curve-handle" }, cx: "{chart_x(month)}", cy: "{chart_y(current_values[month as usize - 1])}", r: if active_month() == month { "7" } else { "5" }, fill: "{chart_active_color}" }
                                }
                                rect { class: "curve-drag-surface", x: "0", y: "45", width: "900", height: "955", fill: "transparent",
                                    onpointerdown: move |event| {
                                        if saving() { return; }
                                        event.prevent_default();
                                        line_picker.set(None);
                                        press_moved.set(false);
                                        if let (Some((month, _)), Some((x, y))) = (chart_pointer(&event), chart_coordinates(&event)) {
                                            let position = event.client_coordinates();
                                            let hits = nearby_lines(&properties(), &visible_houses(), &visible_rooms(), x, y);
                                            if hits.is_empty() {
                                                panning.set(Some((position.x, position.y)));
                                                press_hits.set(None);
                                            } else {
                                                if hits.contains(&active()) && (x - chart_x(month)).abs() <= 10.0 && (y - chart_y(active_values(&properties(), active())[month as usize - 1])).abs() <= 9.0 {
                                                    dragging.set(Some(month));
                                                }
                                                press_hits.set(Some((month, hits, position.x, position.y)));
                                            }
                                        }
                                    },
                                    onpointermove: move |event| {
                                        let position = event.client_coordinates();
                                        if let Some((_, _, start_x, start_y)) = press_hits() {
                                            if (position.x - start_x).abs() + (position.y - start_y).abs() > 5.0 { press_moved.set(true); }
                                        }
                                        if let Some(month) = dragging() {
                                            if press_moved() {
                                                active_month.set(month);
                                                cursor_day.set(day_of_year(chart_year, month, 15));
                                                if let Some((_, percent)) = chart_pointer(&event) { adjust_curve(properties, dirty, active(), month, percent); }
                                            }
                                        } else if let Some((previous_x, previous_y)) = panning() {
                                            if let Some(scroller) = web_sys::window().and_then(|window| window.document()).and_then(|document| document.query_selector("#pricing-curve-scroll").ok().flatten()) {
                                                scroller.set_scroll_left(scroller.scroll_left() + (previous_x - position.x) as i32);
                                                scroller.set_scroll_top(scroller.scroll_top() + (previous_y - position.y) as i32);
                                            }
                                            panning.set(Some((position.x, position.y)));
                                        }
                                    },
                                    onpointerup: move |_| {
                                        if !press_moved() {
                                            if let Some((month, hits, _, _)) = press_hits() {
                                                active_month.set(month);
                                                cursor_day.set(day_of_year(chart_year, month, 15));
                                                if hits.len() == 1 { active.set(hits[0]); }
                                                else { line_picker.set(Some((month, hits))); }
                                            }
                                        }
                                        dragging.set(None); panning.set(None); press_hits.set(None);
                                    },
                                    onpointerleave: move |_| { dragging.set(None); panning.set(None); press_hits.set(None); },
                                    onpointercancel: move |_| { dragging.set(None); panning.set(None); press_hits.set(None); },
                                }
                                rect { class: "curve-cursor-surface", x: "0", y: "0", width: "900", height: "45", fill: "transparent",
                                    onpointerdown: move |event| { event.prevent_default(); cursor_dragging.set(true); if let Some((x, _)) = chart_coordinates(&event) { cursor_day.set(day_from_chart_x(chart_year, x)); } },
                                    onpointermove: move |event| { if cursor_dragging() { if let Some((x, _)) = chart_coordinates(&event) { cursor_day.set(day_from_chart_x(chart_year, x)); } } },
                                    onpointerup: move |_| cursor_dragging.set(false),
                                    onpointerleave: move |_| cursor_dragging.set(false),
                                    onpointercancel: move |_| cursor_dragging.set(false),
                                }
                            }
                        }
                        if let Some((month, choices)) = line_picker() {
                            div { class: "curve-line-picker", role: "dialog", aria_label: "Choose overlapping pricing line",
                                div { class: "curve-line-picker-head", strong { "Select line · {MONTHS[month as usize - 1]}" } button { aria_label: "Close line picker", onclick: move |_| line_picker.set(None), "×" } }
                                for choice in choices {
                                    {
                                        let label = line_label(&all, choice);
                                        let color = match choice { ActiveLine::Portfolio => "#68AA72", ActiveLine::House(id) | ActiveLine::Room(id, _) => house_color(id) };
                                        rsx! { button { class: if choice == current_active { "active" } else { "" }, style: "--line-color: {color}", onclick: move |_| { active.set(choice); active_month.set(month); line_picker.set(None); }, span { class: "curve-line-choice-swatch" } "{label}" } }
                                    }
                                }
                            }
                        }
                    }
                    if config_open() { section { class: "curve-house-panel",
                        div { class: "curve-house-top",
                            div { class: "curve-house-title", h2 { "Houses & rooms" } button { aria_label: "Collapse houses and rooms", onclick: move |_| config_open.set(false), "‹" } }
                            p { class: "curve-viewed-month", "Viewing {viewed_label} prices" }
                            input { class: "search-input", aria_label: "Search houses and rooms", placeholder: "Search house or room", value: "{search}", oninput: move |event| search.set(event.value()) }
                            div { class: "curve-price-columns", span { "BASE" } span { "CURRENT" } span { "CURSOR · {viewed_label}" } }
                        }
                        for house in all.iter().filter(|house| query.is_empty() || house.id.to_string().contains(&query) || house.name.to_lowercase().contains(&query) || house.address.to_lowercase().contains(&query) || house.spaces.iter().any(|room| room.name.to_lowercase().contains(&query))) {
                            div { key: "{house.id}", class: "curve-house-group", style: "--house-color: {house_color(house.id)}",
                                div { class: "curve-house-head",
                                    HouseBadge { id: house.id }
                                    button { class: if current_active == ActiveLine::House(house.id) { "curve-select active" } else { "curve-select" }, onclick: { let house_id = house.id; move |_| { active.set(ActiveLine::House(house_id)); line_picker.set(None); if !visible_houses().contains(&house_id) { visible_houses.write().push(house_id); } } }, "{house.address}" }
                                    label { class: "curve-toggle", title: "Show house line", input { r#type: "checkbox", aria_label: "Show line for house {house.id}", checked: visible_houses().contains(&house.id), onchange: { let house_id = house.id; move |event| { if event.checked() { if !visible_houses().contains(&house_id) { visible_houses.write().push(house_id); } active.set(ActiveLine::House(house_id)); } else { visible_houses.write().retain(|id| *id != house_id); if active() == ActiveLine::House(house_id) { active.set(ActiveLine::Portfolio); } } } } } }
                                }
                                div { class: "curve-house-total", "C${house_price_at(house, chart_year, viewed_day)} / month across rooms" }
                                for room in house.spaces.iter().filter(|room| (room.kind == "Bedroom" || room.kind == "Suite") && (query.is_empty() || house.id.to_string().contains(&query) || house.name.to_lowercase().contains(&query) || house.address.to_lowercase().contains(&query) || room.name.to_lowercase().contains(&query))) {
                                    div { key: "{room.id}", class: "curve-room-row",
                                        label { class: "curve-toggle", title: "Show room line", input { r#type: "checkbox", aria_label: "Show line for {room.name} in house {house.id}", checked: visible_rooms().contains(&(house.id, room.id)), onchange: { let house_id = house.id; let room_id = room.id; move |event| { if event.checked() { if !visible_rooms().contains(&(house_id, room_id)) { visible_rooms.write().push((house_id, room_id)); } active.set(ActiveLine::Room(house_id, room_id)); } else { visible_rooms.write().retain(|item| *item != (house_id, room_id)); if active() == ActiveLine::Room(house_id, room_id) { active.set(ActiveLine::Portfolio); } } } } } }
                                        button { class: if current_active == ActiveLine::Room(house.id, room.id) { "curve-select active" } else { "curve-select" }, onclick: { let house_id = house.id; let room_id = room.id; move |_| { active.set(ActiveLine::Room(house_id, room_id)); line_picker.set(None); if !visible_rooms().contains(&(house_id, room_id)) { visible_rooms.write().push((house_id, room_id)); } } }, "{room.name}" }
                                        label { class: "curve-base-field", span { "$" } input { r#type: "number", min: "1", aria_label: "Base monthly rent for {room.name} in house {house.id}", value: room.base_rent().map(|value| value.to_string()).unwrap_or_default(), oninput: { let house_id = house.id; let room_id = room.id; move |event| edit_base(properties, dirty, house_id, room_id, event.value()) } } }
                                        label { class: "curve-current-price curve-day-field", span { "$" } input { r#type: "number", min: "1", aria_label: "Current price for {room.name} in house {house.id} on {today_label}", value: price_draft().filter(|(house_id, room_id, day, _)| *house_id == house.id && *room_id == room.id && *day == today_day).map(|(_, _, _, value)| value).or_else(|| room_price_at(room, chart_year, today_day).map(|price| price.to_string())).unwrap_or_default(), oninput: { let house_id = house.id; let room_id = room.id; move |event| price_draft.set(Some((house_id, room_id, today_day, event.value()))) }, onblur: { let house_id = house.id; let room_id = room.id; move |_| { if let Some((draft_house, draft_room, day, value)) = price_draft() { if draft_house == house_id && draft_room == room_id { edit_day_price(properties, dirty, house_id, room_id, chart_year, day, value); price_draft.set(None); } } } } } }
                                        label { class: "curve-current-price curve-day-field", span { "$" } input { r#type: "number", min: "1", aria_label: "Cursor price for {room.name} in house {house.id} on {viewed_label}", value: price_draft().filter(|(house_id, room_id, day, _)| *house_id == house.id && *room_id == room.id && *day == viewed_day).map(|(_, _, _, value)| value).or_else(|| room_price_at(room, chart_year, viewed_day).map(|price| price.to_string())).unwrap_or_default(), oninput: { let house_id = house.id; let room_id = room.id; move |event| price_draft.set(Some((house_id, room_id, cursor_day(), event.value()))) }, onblur: { let house_id = house.id; let room_id = room.id; move |_| { if let Some((draft_house, draft_room, day, value)) = price_draft() { if draft_house == house_id && draft_room == room_id { edit_day_price(properties, dirty, house_id, room_id, chart_year, day, value); price_draft.set(None); } } } } } }
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
