use dioxus::prelude::*;

pub fn house_color(id: u32) -> &'static str {
    const COLORS: [&str; 8] = [
        "#287A64", "#AD7527", "#4779A6", "#AD655B", "#7968A5", "#608445", "#A56086", "#538C97",
    ];
    COLORS[(id.saturating_sub(1) as usize) % COLORS.len()]
}

#[component]
pub fn HouseBadge(id: u32) -> Element {
    let color = house_color(id);
    rsx! {
        span { class: "house-badge", role: "img", aria_label: "House {id}", style: "color: {color}",
            svg { view_box: "0 0 44 44",
                path { d: "M5 20.5 22 7l17 13.5", fill: "none", stroke: "currentColor", stroke_width: "2.8", stroke_linecap: "round", stroke_linejoin: "round" }
                path { d: "M9 19v18h26V19", fill: "none", stroke: "currentColor", stroke_width: "2.8", stroke_linejoin: "round" }
                text { x: "22", y: "32.5", text_anchor: "middle", fill: "currentColor", font_size: "15", font_weight: "800", "{id}" }
            }
        }
    }
}
