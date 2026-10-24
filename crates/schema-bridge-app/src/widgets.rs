//! Small display pieces shared by several screens: the compact summary
//! cards (a headline number, a segmented bar and a legend) used on the Load
//! and Report stages. They only present numbers from
//! `schema_bridge_core::report::Summary`.

use dioxus::prelude::*;
use schema_bridge_core::Severity;
use schema_bridge_core::report::Summary;

use crate::icons::Icon;

/// One part of a summary bar.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub label: &'static str,
    pub count: usize,
    /// `success`, `warning`, `high`, `error`, `info` or `neutral`.
    pub tone: &'static str,
}

/// What one summary card shows.
#[derive(Debug, Clone, PartialEq)]
pub struct SummaryData {
    pub title: &'static str,
    pub icon: &'static str,
    pub headline: String,
    pub caption: String,
    pub segments: Vec<Segment>,
}

fn seg(label: &'static str, count: usize, tone: &'static str) -> Segment {
    Segment { label, count, tone }
}

/// Tables, columns and problems as summary cards. The first two need both
/// databases.
pub fn summary_cards(s: &Summary, both: bool) -> Vec<SummaryData> {
    let mut out = Vec::new();
    if both {
        let t = s.tables;
        out.push(SummaryData {
            title: "Tables",
            icon: "load",
            headline: format!("{} of {}", t.matched, t.total),
            caption: "source tables have a place in the target".into(),
            // Uncertain tables are always matched ones, so the matched
            // tables are split into confident and to review.
            segments: vec![
                seg(
                    "confident",
                    t.matched.saturating_sub(t.uncertain.min(t.matched)),
                    "success",
                ),
                seg("to review", t.uncertain.min(t.matched), "warning"),
                seg("no place yet", t.no_place, "info"),
                seg("stay in source", t.stays, "neutral"),
            ],
        });
        let c = s.columns;
        out.push(SummaryData {
            title: "Columns",
            icon: "matching",
            headline: format!("{} of {}", c.mapped, c.total),
            caption: "source columns are mapped".into(),
            segments: vec![
                seg("same or safe", c.by_fit[0] + c.by_fit[1], "success"),
                seg("conversion", c.requiring_conversion(), "warning"),
                seg("may lose data", c.with_possible_loss(), "high"),
                seg("incompatible", c.incompatible(), "error"),
                seg("not mapped", c.not_mapped + c.never_copied, "neutral"),
            ],
        });
    }
    let p = |sev: Severity| s.total_problems(sev);
    let total: usize = Severity::ALL.iter().map(|sev| p(*sev)).sum();
    out.push(SummaryData {
        title: "Problems",
        icon: "problems",
        headline: total.to_string(),
        caption: if both {
            "inside each database and in fitting them together".into()
        } else {
            "found in the loaded database".into()
        },
        segments: vec![
            seg("blocking", p(Severity::Blocking), "error"),
            seg("high", p(Severity::High), "high"),
            seg("medium", p(Severity::Medium), "warning"),
            seg("low / info", p(Severity::Low) + p(Severity::Info), "info"),
        ],
    });
    out
}

/// A compact, scannable summary: title, headline, a bar split by tone and
/// the counts behind it ("12 matched · 2 uncertain · 1 stays").
#[component]
pub fn SummaryCard(data: SummaryData) -> Element {
    let SummaryData {
        title,
        icon,
        headline,
        caption,
        segments,
    } = data;
    let total: usize = segments.iter().map(|s| s.count).sum();
    let bars: Vec<(String, String, String)> = segments
        .iter()
        .filter(|s| s.count > 0)
        .map(|s| {
            (
                format!("sbar tone-{}", s.tone),
                format!("flex-grow: {}", s.count),
                format!("{} {}", s.count, s.label),
            )
        })
        .collect();
    let legend: Vec<(String, String, &'static str, bool)> = segments
        .iter()
        .map(|s| {
            (
                format!("sleg tone-{}", s.tone),
                s.count.to_string(),
                s.label,
                s.count == 0,
            )
        })
        .collect();
    rsx! {
        section { class: "summary-card",
            div { class: "summary-top",
                span { class: "summary-icon", Icon { name: icon, size: 16 } }
                h3 { {title} }
            }
            p { class: "summary-headline",
                span { class: "summary-number", {headline} }
                span { class: "summary-caption", {caption} }
            }
            div {
                class: "summary-bar",
                role: "img",
                aria_label: "{title}: {total} in total",
                for (class , style , label) in bars {
                    span { key: "{label}", class, style, title: "{label}" }
                }
            }
            ul { class: "summary-legend",
                for (class , count , label , zero) in legend {
                    li { key: "{label}", class: if zero { "zero" } else { "" },
                        span { class, aria_hidden: "true" }
                        b { {count} }
                        " {label}"
                    }
                }
            }
        }
    }
}
