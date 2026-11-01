use dioxus::prelude::*;

#[component]
pub fn AdminShell(
    active: &'static str,
    main_class: &'static str,
    property_count: Option<usize>,
    children: Element,
) -> Element {
    let _ = (active, property_count);
    rsx! {
        main { id: "main-content", tabindex: "-1", class: "{main_class}", {children} }
    }
}
