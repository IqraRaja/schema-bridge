//! The "Report" stage: the whole comparison on one screen, for someone who
//! did not run it. Counts and lists come from `schema_bridge_core::report`;
//! the same content can be saved as an HTML or Markdown report.

use dioxus::prelude::*;
use schema_bridge_core::report::{report_html, report_markdown, summary};
use schema_bridge_core::workspace::{Workspace, system_name};

use crate::browser::{download_text, file_stem};
use crate::icons::Icon;
use crate::problems::severity_class;
use crate::state::derived;
use crate::widgets::{SummaryCard, summary_cards};

/// Unresolved items of one kind: (kind, what it means, (item, detail) pairs).
type UnresolvedGroup = (&'static str, &'static str, Vec<(String, String)>);

#[component]
pub fn ReportStage(
    workspace: Signal<Workspace>,
    go_to_load: EventHandler<()>,
    go_to_matching: EventHandler<()>,
    go_to_export: EventHandler<()>,
) -> Element {
    let d = derived();
    let ws = workspace.read();
    let loaded = ws.source.schema.is_some() || ws.target.schema.is_some();
    let head = rsx! {
        section { class: "page-head",
            div {
                h2 { "Comparison report" }
                p { class: "lead",
                    "The whole comparison on one page: what matches, what needs converting, what is wrong, and what is still open. Save it as a report to share."
                }
            }
        }
    };
    if !loaded {
        drop(ws);
        return rsx! {
            {head}
            section { class: "card empty",
                p { class: "empty-title", "Nothing to report yet" }
                p { "Load a source and a target export, or try the sample schemas." }
                div {
                    button { class: "btn primary", onclick: move |_| go_to_load.call(()), "Go to Load schemas" }
                }
            }
        };
    }

    let analysis = d.analysis.read();
    let s = summary(&ws, analysis.as_ref());
    let html = report_html(&ws, analysis.as_ref());
    let markdown = report_markdown(&ws, analysis.as_ref());
    let both = analysis.is_some();
    let system = |schema: &Option<schema_bridge_core::Schema>| {
        schema.as_ref().map_or("not loaded".to_string(), |x| {
            system_name(&x.dbms).to_string()
        })
    };
    let (source_system, target_system) = (system(&ws.source.schema), system(&ws.target.schema));
    let stem = format!("{}-into-{}", file_stem(&s.source), file_stem(&s.target));
    drop(analysis);
    drop(ws);

    // Blocking problems inside a database, and open decisions (a blocking
    // integration problem is itself one of the decisions, so it is not
    // counted twice).
    let count = |kind: &str| s.unresolved.iter().filter(|u| u.kind == kind).count();
    let (fix, decide, review) = (count("Fix first"), count("Decide"), count("Review"));
    let open = fix + decide;
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let (verdict_class, verdict_title, verdict_text) = if !both {
        (
            "verdict info",
            "Only one database loaded".to_string(),
            "Load the other database to see matching, risks and the plan.".to_string(),
        )
    } else if open == 0 {
        (
            "verdict success",
            "Ready for a test load".to_string(),
            format!(
                "Nothing blocks a first test load on a test copy. {} could still be reviewed.",
                plural(review, "match", "matches")
            ),
        )
    } else {
        (
            "verdict error",
            format!(
                "{} to resolve before a test load",
                plural(open, "item", "items")
            ),
            format!(
                "{} and {}. See the list below.",
                plural(fix, "blocking schema finding", "blocking schema findings"),
                plural(decide, "open decision", "open decisions")
            ),
        )
    };

    let cards = summary_cards(&s, both);
    let risks: Vec<(String, &'static str, String, String)> = s
        .risks
        .iter()
        .map(|f| {
            (
                format!("pill {}", severity_class(f.severity)),
                f.severity.label(),
                f.title.clone(),
                f.object.clone(),
            )
        })
        .collect();
    let groups: Vec<UnresolvedGroup> = [
        ("Fix first", "Blocking schema findings"),
        ("Decide", "Needs people who know the data"),
        ("Review", "Uncertain automatic matches"),
        ("Draft SQL", "Open markers in the draft"),
    ]
    .into_iter()
    .map(|(kind, hint)| {
        (
            kind,
            hint,
            s.unresolved
                .iter()
                .filter(|u| u.kind == kind)
                .map(|u| (u.item.clone(), u.detail.clone()))
                .collect::<Vec<_>>(),
        )
    })
    .filter(|(_, _, items)| !items.is_empty())
    .collect();
    let (html_name, md_name) = (format!("{stem}-report.html"), format!("{stem}-report.md"));

    rsx! {
        {head}
        div { class: "report-actions",
            button {
                class: "btn primary",
                onclick: move |_| download_text(&html_name, "text/html", html.clone()),
                Icon { name: "export", size: 15 }
                "Save report (HTML)"
            }
            button {
                class: "btn secondary",
                onclick: move |_| download_text(&md_name, "text/markdown", markdown.clone()),
                "Save as Markdown"
            }
            button { class: "btn ghost", onclick: move |_| go_to_export.call(()), "More export options" }
        }
        section { class: "card report-hero",
            div { class: "hero-pair",
                div { class: "hero-db src",
                    span { class: "hero-role", "Source" }
                    span { class: "hero-name", {s.source.clone()} }
                    span { class: "hero-system", {source_system} }
                }
                span { class: "hero-arrow", aria_hidden: "true", Icon { name: "arrow", size: 22 } }
                div { class: "hero-db tgt",
                    span { class: "hero-role", "Target" }
                    span { class: "hero-name", {s.target.clone()} }
                    span { class: "hero-system", {target_system} }
                }
            }
            div { class: verdict_class, role: "status",
                b { {verdict_title} }
                span { {verdict_text} }
            }
        }
        section { class: "analysis", aria_label: "The comparison in numbers",
            for card in cards {
                SummaryCard { key: "{card.title}", data: card }
            }
        }
        if both {
            section { class: "card",
                div { class: "card-head",
                    h3 { "Integration risks" }
                    p { class: "muted", "Blocking and high integration issues." }
                }
                if risks.is_empty() {
                    p { class: "plan-ok", "No blocking or high risks." }
                } else {
                    ul { class: "plan-items",
                        for (i , (pill , sev , title , object)) in risks.into_iter().enumerate() {
                            li { key: "{i}",
                                span { class: pill, {sev} }
                                span { class: "plan-item-text",
                                    span { class: "plan-item-title", {title} }
                                    span { class: "plan-item-object mono", {object} }
                                }
                            }
                        }
                    }
                }
            }
        }
        section { class: "card",
            div { class: "card-head row",
                div {
                    h3 { "Unresolved items" }
                    p { class: "muted", "What someone still has to fix, decide or review." }
                }
                if review > 0 {
                    button { class: "btn secondary small", onclick: move |_| go_to_matching.call(()), "Review matches" }
                }
            }
            if groups.is_empty() {
                p { class: "plan-ok", "Nothing is left open." }
            }
            for (kind , hint , items) in groups {
                div { key: "{kind}", class: "unresolved-group",
                    p { class: "unresolved-head",
                        b { {kind} }
                        span { class: "muted", " · {hint} · {items.len()}" }
                    }
                    ul { class: "unresolved",
                        for (i , (item , detail)) in items.into_iter().enumerate() {
                            li { key: "{i}",
                                span { class: "mono", {item} }
                                span { class: "muted", {detail} }
                            }
                        }
                    }
                }
            }
        }
    }
}
