use dioxus::prelude::*;
use dioxus_icons::lucide::{
    ArrowLeft, ArrowRight, ArrowUpRight, CalendarDays, Check, ChevronDown, ChevronLeft,
    ChevronRight, Clock3, Copy, CornerDownLeft, FileText, House, Mail, Minus, Phone, Plus, Redo2,
    Search, Trash2, Undo2, UsersRound, X,
};

#[component]
pub fn Icon(name: &'static str) -> Element {
    match name {
        "plus" => rsx! { Plus { class: "ui-icon" } },
        "search" => rsx! { Search { class: "ui-icon" } },
        "minus" => rsx! { Minus { class: "ui-icon" } },
        "trash" => rsx! { Trash2 { class: "ui-icon" } },
        "close" => rsx! { X { class: "ui-icon" } },
        "chevron-left" => rsx! { ChevronLeft { class: "ui-icon" } },
        "chevron-right" => rsx! { ChevronRight { class: "ui-icon" } },
        "chevron-down" => rsx! { ChevronDown { class: "ui-icon" } },
        "arrow-left" => rsx! { ArrowLeft { class: "ui-icon" } },
        "arrow-right" => rsx! { ArrowRight { class: "ui-icon" } },
        "arrow-up-right" => rsx! { ArrowUpRight { class: "ui-icon" } },
        "check" => rsx! { Check { class: "ui-icon" } },
        "copy" => rsx! { Copy { class: "ui-icon" } },
        "return" => rsx! { CornerDownLeft { class: "ui-icon" } },
        "undo" => rsx! { Undo2 { class: "ui-icon" } },
        "redo" => rsx! { Redo2 { class: "ui-icon" } },
        "calendar" => rsx! { CalendarDays { class: "ui-icon" } },
        "house" => rsx! { House { class: "ui-icon" } },
        "users" => rsx! { UsersRound { class: "ui-icon" } },
        "clock" => rsx! { Clock3 { class: "ui-icon" } },
        "phone" => rsx! { Phone { class: "ui-icon" } },
        "file" => rsx! { FileText { class: "ui-icon" } },
        "mail" => rsx! { Mail { class: "ui-icon" } },
        _ => rsx! {},
    }
}
