use crate::icon::Icon;
use crate::{api, model::Property};
use chrono::NaiveDate;
use dioxus::prelude::*;
use serde::Deserialize;

#[derive(Clone, Deserialize)]
struct Occupancy {
    id: i64,
    tenant_name: String,
    property_id: i64,
    room_id: i64,
    lease_type: String,
    status: String,
    details: StayDates,
}

#[derive(Clone, Deserialize)]
struct StayDates {
    start_date: String,
    end_date: String,
}

fn month_at(index: i32) -> (i32, u32) {
    (index.div_euclid(12), index.rem_euclid(12) as u32 + 1)
}

fn occupancy_in_month(lease: &Occupancy, index: i32) -> bool {
    if lease.status != "signed" && lease.status != "confirmed" {
        return false;
    }
    let Ok(start) = NaiveDate::parse_from_str(&lease.details.start_date, "%Y-%m-%d") else {
        return false;
    };
    let (year, month) = month_at(index);
    let first = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .unwrap();
    let end = NaiveDate::parse_from_str(&lease.details.end_date, "%Y-%m-%d").ok();
    start < next && end.is_none_or(|date| date >= first)
}

struct StaySpan<'a> {
    lease: &'a Occupancy,
    start: usize,
    length: usize,
    lane: usize,
}

fn room_spans<'a>(
    leases: &'a [Occupancy],
    property_id: i64,
    room_id: i64,
    first: i32,
    count: usize,
) -> Vec<StaySpan<'a>> {
    let mut spans: Vec<_> = leases
        .iter()
        .filter(|lease| {
            lease.property_id == property_id
                && (lease.lease_type == "entire_house" || lease.room_id == room_id)
        })
        .filter_map(|lease| {
            let months: Vec<_> = (0..count)
                .filter(|offset| occupancy_in_month(lease, first + *offset as i32))
                .collect();
            Some(StaySpan {
                lease,
                start: *months.first()?,
                length: months.len(),
                lane: 0,
            })
        })
        .collect();
    spans.sort_by_key(|span| (span.start, span.lease.id));
    let mut lane_ends: Vec<usize> = Vec::new();
    for span in &mut spans {
        let end = span.start + span.length;
        if let Some(lane) = lane_ends
            .iter()
            .position(|occupied_until| *occupied_until <= span.start)
        {
            span.lane = lane;
            lane_ends[lane] = end;
        } else {
            span.lane = lane_ends.len();
            lane_ends.push(end);
        }
    }
    spans
}

fn year_groups(months: &[i32]) -> Vec<(i32, usize, usize)> {
    let mut groups = Vec::new();
    for (offset, index) in months.iter().enumerate() {
        let year = month_at(*index).0;
        if let Some((last_year, _, length)) = groups.last_mut() {
            if *last_year == year {
                *length += 1;
                continue;
            }
        }
        groups.push((year, offset + 2, 1));
    }
    groups
}

pub fn vacancy_page() -> Element {
    const VISIBLE_MONTHS: usize = 42;
    const SCROLL_STEP: i32 = 12;
    const MONTH_WIDTH: i32 = 96;
    let today = js_sys::Date::new_0();
    let current_month = today.get_full_year() as i32 * 12 + today.get_month() as i32;
    let current_day = today.get_date();
    let mut first_month = use_signal(|| current_month - 6);
    let mut scroll_shift = use_signal(|| 0_i32);
    let mut last_scroll_left = use_signal(|| 0_i32);
    let mut properties = use_signal(Vec::<Property>::new);
    let mut occupancy = use_signal(Vec::<Occupancy>::new);
    let mut occupancy_ready = use_signal(|| false);
    let mut loading = use_signal(|| true);
    let mut message = use_signal(String::new);
    use_effect(move || {
        spawn(async move {
            match api::list().await {
                Ok(data) => properties.set(data),
                Err(error) => message.set(error),
            }
            match gloo_net::http::Request::get("/api/leases").send().await {
                Ok(response) if response.ok() => match response.json::<Vec<Occupancy>>().await {
                    Ok(data) => {
                        occupancy.set(data);
                        occupancy_ready.set(true);
                    }
                    Err(_) => message
                        .set("Could not read occupancy. Vacancy cannot be shown reliably.".into()),
                },
                _ => message
                    .set("Could not load occupancy. Vacancy cannot be shown reliably.".into()),
            }
            loading.set(false);
        });
    });
    use_effect(move || {
        let _ = first_month();
        let shift = scroll_shift();
        if shift == 0 {
            return;
        }
        if let Some(scroller) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| {
                document
                    .query_selector("#vacancy-grid-scroll")
                    .ok()
                    .flatten()
            })
        {
            let position = scroller.scroll_left() + shift;
            last_scroll_left.set(position);
            scroller.set_scroll_left(position);
        }
        scroll_shift.set(0);
    });

    let leases = occupancy();
    let houses = properties();
    let months: Vec<i32> = (0..VISIBLE_MONTHS)
        .map(|offset| first_month() + offset as i32)
        .collect();
    let grid_style = format!(
        "grid-template-columns: minmax(190px, 190px) repeat({}, minmax(96px, 96px));",
        months.len()
    );
    let years = year_groups(&months);
    let today_offset = current_month - first_month();
    let today_line = if today_offset >= 0 && (today_offset as usize) < months.len() {
        let (year, month) = month_at(current_month);
        let first = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
        let next = if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(year, month + 1, 1)
        }
        .unwrap();
        let days = (next - first).num_days() as f64;
        Some(190.0 + (today_offset as f64 + (current_day as f64 - 0.5) / days) * 96.0)
    } else {
        None
    };

    rsx! {
        document::Title { "Vacancy | Buildry" }
        document::Stylesheet { href: crate::CSS }
        crate::admin_shell::AdminShell { active: "/vacancy", property_count: None, main_class: "main-content vacancy-admin vacancy-overview",
            section { class: "page-head",
                h1 { "Vacancy" }
                a { class: "vacancy-create-link round-add-action", href: "/leases/new", aria_label: "Create lease", title: "Create lease", Icon { name: "plus" } span { "Create lease" } }
            }
            if loading() { p { class: "stays-state", "Loading vacancies…" } }
            if !message().is_empty() { p { class: "availability-message", role: "status", "{message}" } }
            if houses.is_empty() && !loading() { p { class: "stays-state", "No houses are available yet." } }
            if !houses.is_empty() {
                div { id: "vacancy-grid-scroll", class: "vacancy-matrix-scroll vacancy-combined-scroll", tabindex: "0", aria_label: "Monthly vacancies for all houses; scroll horizontally to load earlier or later months",
                    onscroll: move |_| {
                        if scroll_shift() != 0 { return; }
                        let Some(scroller) = web_sys::window()
                            .and_then(|window| window.document())
                            .and_then(|document| document.query_selector("#vacancy-grid-scroll").ok().flatten()) else { return; };
                        let left = scroller.scroll_left();
                        let previous = last_scroll_left();
                        if left == previous || scroller.scroll_width() <= scroller.client_width() { return; }
                        last_scroll_left.set(left);
                        if left < previous && left <= 3 * MONTH_WIDTH {
                            scroll_shift.set(SCROLL_STEP * MONTH_WIDTH);
                            first_month.set(first_month() - SCROLL_STEP);
                        } else if left > previous && scroller.scroll_width() - scroller.client_width() - left <= 3 * MONTH_WIDTH {
                            scroll_shift.set(-SCROLL_STEP * MONTH_WIDTH);
                            first_month.set(first_month() + SCROLL_STEP);
                        }
                    },
                    div { class: "vacancy-matrix",
                        div { class: "vacancy-year-header", style: "{grid_style}",
                            span { "Year" }
                            for (year, column, length) in &years {
                                span { key: "{year}", style: "grid-column: {column} / span {length};", "{year}" }
                            }
                        }
                        div { class: "vacancy-matrix-header", style: "{grid_style}", span { "House / room" } for month in &months { span { key: "{month}", class: if *month == current_month { "current" } else { "" }, "{month_at(*month).1}" } } }
                        for house in houses {
                            div { key: "{house.id}", class: "vacancy-house-divider", style: "{grid_style}",
                                div { class: "vacancy-house-name", strong { "{house.name}" } }
                            }
                            for room in house.spaces.iter().filter(|room| room.kind == "Bedroom" || room.kind == "Suite") {
                                {
                                    let spans = room_spans(&leases, house.id as i64, room.id as i64, first_month(), months.len());
                                    let lanes = spans.iter().map(|span| span.lane + 1).max().unwrap_or(1);
                                    let row_style = format!("{grid_style} grid-template-rows:repeat({lanes}, 56px);");
                                    rsx! {
                                        div { key: "{house.id}-{room.id}", class: "vacancy-matrix-row vacancy-row", style: "{row_style}",
                                            div { class: "vacancy-matrix-room", title: "{room.name}", strong { "{room.name}" } }
                                            for (offset, month) in months.iter().enumerate() {
                                                {
                                                    let occupied = spans.iter().any(|span| offset >= span.start && offset < span.start + span.length);
                                                    let cell_style = format!("grid-column:{};", offset + 2);
                                                    let (year, number) = month_at(*month);
                                                    let bookable = !occupied && occupancy_ready() && *month >= current_month;
                                                    let day = if *month == current_month { current_day } else { 1 };
                                                    let booking_link = format!("/leases/new?property={}&room={}&start={year}-{number:02}-{day:02}", house.id, room.id);
                                                    if bookable {
                                                        rsx! { a { key: "{month}", class: "vacancy-month open", style: "{cell_style}", href: "{booking_link}",
                                                            aria_label: "Create a lease for {room.name} in {year}-{number:02}",
                                                            title: "Create a lease for {room.name} in {year}-{number:02}"
                                                        } }
                                                    } else {
                                                        let cell_class = if occupied { "vacancy-month reserved" } else if !occupancy_ready() { "vacancy-month unknown" } else { "vacancy-month past" };
                                                        rsx! { div { key: "{month}", class: "{cell_class}", style: "{cell_style}",
                                                            if !occupied && !occupancy_ready() { strong { "Unknown" } }
                                                        } }
                                                    }
                                                }
                                            }
                                            for span in spans {
                                                {
                                                    let column = span.start + 2;
                                                    let bar_style = format!("grid-column:{column} / span {}; grid-row:{};", span.length, span.lane + 1);
                                                    let end_label = if span.lease.details.end_date.is_empty() { "Ongoing" } else { &span.lease.details.end_date };
                                                    let status_label = if span.lease.status == "signed" { "Signed" } else { "Confirmed" };
                                                    rsx! { a { key: "{span.lease.id}", class: "vacancy-stay", style: "{bar_style}", href: "/leases/view?lease={span.lease.id}",
                                                        title: "{span.lease.tenant_name}: {span.lease.details.start_date} to {end_label}",
                                                        strong { "{span.lease.tenant_name}" }
                                                        small { "{span.lease.details.start_date} – {end_label} · {status_label}" }
                                                    } }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if house.spaces.iter().all(|room| room.kind != "Bedroom" && room.kind != "Suite") { p { class: "empty", "No rooms or suites yet." } }
                        }
                        if let Some(left) = today_line {
                            div { class: "vacancy-today-line", style: "left:{left}px;", aria_hidden: "true" }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{month_at, occupancy_in_month, room_spans, year_groups, Occupancy, StayDates};

    #[test]
    fn month_boundary_and_partial_stays() {
        let lease = Occupancy {
            id: 1,
            tenant_name: "Test".into(),
            property_id: 1,
            room_id: 1,
            lease_type: "room".into(),
            status: "confirmed".into(),
            details: StayDates {
                start_date: "2025-12-15".into(),
                end_date: "2026-01-01".into(),
            },
        };
        assert_eq!(month_at(2025 * 12 + 11), (2025, 12));
        assert!(occupancy_in_month(&lease, 2025 * 12 + 11));
        assert!(occupancy_in_month(&lease, 2026 * 12));
        assert!(!occupancy_in_month(&lease, 2026 * 12 + 1));
    }

    #[test]
    fn stays_span_months_and_overlaps_use_separate_lanes() {
        let leases = vec![
            Occupancy {
                id: 1,
                tenant_name: "One".into(),
                property_id: 1,
                room_id: 1,
                lease_type: "room".into(),
                status: "confirmed".into(),
                details: StayDates {
                    start_date: "2026-11-15".into(),
                    end_date: "2027-02-10".into(),
                },
            },
            Occupancy {
                id: 2,
                tenant_name: "Two".into(),
                property_id: 1,
                room_id: 1,
                lease_type: "room".into(),
                status: "signed".into(),
                details: StayDates {
                    start_date: "2027-02-01".into(),
                    end_date: "2027-03-31".into(),
                },
            },
        ];
        let first = 2026 * 12 + 9;
        let spans = room_spans(&leases, 1, 1, first, 6);
        assert_eq!((spans[0].start, spans[0].length, spans[0].lane), (1, 4, 0));
        assert_eq!((spans[1].start, spans[1].length, spans[1].lane), (4, 2, 1));
        assert_eq!(
            year_groups(&(first..first + 6).collect::<Vec<_>>()),
            vec![(2026, 2, 3), (2027, 5, 3)]
        );
    }

    #[test]
    fn entire_house_lease_covers_every_room_in_its_house() {
        let leases = vec![Occupancy {
            id: 3,
            tenant_name: "House tenant".into(),
            property_id: 3,
            room_id: 0,
            lease_type: "entire_house".into(),
            status: "confirmed".into(),
            details: StayDates {
                start_date: "2026-12-01".into(),
                end_date: "2027-11-30".into(),
            },
        }];
        let first = 2026 * 12 + 11;
        for room_id in [1, 2, 3, 4, 5] {
            let spans = room_spans(&leases, 3, room_id, first, 12);
            assert_eq!(
                (spans[0].start, spans[0].length, spans[0].lease.id),
                (0, 12, 3)
            );
        }
        assert!(room_spans(&leases, 4, 1, first, 12).is_empty());
    }
}
