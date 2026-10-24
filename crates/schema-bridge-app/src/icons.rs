//! A small set of line icons, drawn inline so the app makes no external
//! requests. Decorative: they are hidden from screen readers, and every
//! icon sits next to a text label.

use dioxus::prelude::*;

fn paths(name: &str) -> &'static str {
    match name {
        "load" => {
            r#"<ellipse cx="12" cy="5" rx="8" ry="3"/><path d="M4 5v14c0 1.7 3.6 3 8 3s8-1.3 8-3V5"/><path d="M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3"/>"#
        }
        "problems" => {
            r#"<path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z"/><path d="M12 9v4"/><path d="M12 17h.01"/>"#
        }
        "matching" => {
            r#"<circle cx="5" cy="6" r="3"/><path d="M12 6h5a2 2 0 0 1 2 2v7"/><path d="m15 9-3-3 3-3"/><circle cx="19" cy="18" r="3"/><path d="M12 18H7a2 2 0 0 1-2-2V9"/><path d="m9 15 3 3-3 3"/>"#
        }
        "integration" => {
            r#"<path d="M9 17H7A5 5 0 0 1 7 7h2"/><path d="M15 7h2a5 5 0 1 1 0 10h-2"/><path d="M8 12h8"/>"#
        }
        "plan" => {
            r#"<path d="m3 17 2 2 4-4"/><path d="m3 7 2 2 4-4"/><path d="M13 6h8"/><path d="M13 12h8"/><path d="M13 18h8"/>"#
        }
        "sql" => r#"<path d="m16 18 6-6-6-6"/><path d="m8 6-6 6 6 6"/>"#,
        "report" => {
            r#"<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v5h6"/><path d="M8 13h2"/><path d="M8 17h8"/><path d="M14 13h2"/>"#
        }
        "export" => {
            r#"<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5"/><path d="M12 15V3"/>"#
        }
        "upload" => {
            r#"<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m17 8-5-5-5 5"/><path d="M12 3v12"/>"#
        }
        "search" => r#"<circle cx="11" cy="11" r="7"/><path d="m21 21-4.3-4.3"/>"#,
        "check" => r#"<path d="M20 6 9 17l-5-5"/>"#,
        "x" => r#"<path d="M18 6 6 18"/><path d="m6 6 12 12"/>"#,
        "undo" => r#"<path d="M3 7v6h6"/><path d="M21 17a9 9 0 0 0-15-6.7L3 13"/>"#,
        "arrow" => r#"<path d="M5 12h14"/><path d="m12 5 7 7-7 7"/>"#,
        "info" => r#"<circle cx="12" cy="12" r="9"/><path d="M12 16v-4"/><path d="M12 8h.01"/>"#,
        "save" => {
            r#"<path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2Z"/><path d="M17 21v-8H7v8"/><path d="M7 3v5h8"/>"#
        }
        "bridge" => {
            r#"<path d="M2 18h20"/><path d="M4 18v-5"/><path d="M20 18v-5"/><path d="M2 13c4-7 16-7 20 0"/><path d="M8 11v7"/><path d="M12 10v8"/><path d="M16 11v7"/>"#
        }
        _ => "",
    }
}

#[component]
pub fn Icon(name: &'static str, #[props(default = 16)] size: u32) -> Element {
    rsx! {
        svg {
            class: "icon",
            width: "{size}",
            height: "{size}",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            "aria-hidden": "true",
            dangerous_inner_html: paths(name),
        }
    }
}
