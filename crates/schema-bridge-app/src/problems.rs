//! The "Schema checks" stage: the findings about each schema, with
//! search and filters by database, severity and rule type. Follows the
//! prototype's Problems tab. The findings and the filter logic come from
//! the core library; this file only prepares strings and places them.

use dioxus::prelude::*;
use schema_bridge_core::workspace::{FindingFilter, Side, Workspace, rule_counts};
use schema_bridge_core::{Finding, Severity, count_by_severity};

use crate::icons::Icon;
use crate::state::derived;

/// Cards shown at first; more on request, so long lists stay fast.
pub const PAGE: usize = 100;

/// CSS class suffix of a severity: `sev-blocking`, `sev-high`, …
pub fn severity_class(severity: Severity) -> String {
    format!("sev-{}", severity.label().to_lowercase())
}

/// Label of a "show more" button.
pub fn more_label(hidden: usize, page: usize) -> String {
    if hidden <= page {
        format!("Show the remaining {hidden}")
    } else {
        format!("Show {page} more ({hidden} not shown)")
    }
}

pub fn pressed(on: bool) -> &'static str {
    if on { "true" } else { "false" }
}

/// One finding card, ready to place.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    key: String,
    class: String,
    pill_class: String,
    severity: &'static str,
    database: String,
    database_class: &'static str,
    area: String,
    rule: &'static str,
    title: String,
    object: String,
    explanation: String,
    fix: String,
}

/// Builds the card of one finding. `database_class` colours the database
/// name (`finding-db src`, `finding-db tgt` or `finding-db both`).
pub fn card(index: usize, f: &Finding, database: String, database_class: &'static str) -> Card {
    Card {
        key: index.to_string(),
        class: format!("finding {}", severity_class(f.severity)),
        pill_class: format!("pill {}", severity_class(f.severity)),
        severity: f.severity.label(),
        database,
        database_class,
        area: f.area.clone(),
        rule: f.rule,
        title: f.title.clone(),
        object: f.object.clone(),
        explanation: f.explanation.clone(),
        fix: f.fix.clone(),
    }
}

/// A list of finding cards.
#[component]
pub fn FindingCards(cards: Vec<Card>) -> Element {
    rsx! {
        section { class: "findings",
            for Card { key , class , pill_class , severity , database , database_class , area , rule , title , object , explanation , fix } in cards {
                article { key: "{key}", class,
                    div { class: "finding-head",
                        span { class: pill_class, {severity} }
                        span { class: database_class, {database} }
                        span { class: "finding-area", {area} }
                        span { class: "finding-rule mono", {rule} }
                    }
                    h3 { class: "finding-title", {title} }
                    p { class: "finding-object mono", {object} }
                    p { class: "finding-text", {explanation} }
                    p { class: "finding-fix",
                        b { "What to do: " }
                        {fix}
                    }
                }
            }
        }
    }
}

/// Search, severity tiles and filters above a list of findings, shared by
/// the Problems and the Integration issues stages. `databases` holds the
/// database buttons (empty for integration issues).
#[component]
pub fn FindingFilters(
    filter: Signal<FindingFilter>,
    /// Severity counts for the current database choice.
    counts: [usize; 5],
    rules: Vec<(&'static str, usize)>,
    databases: Vec<(String, String, Option<Side>)>,
    placeholder: String,
) -> Element {
    let mut filter = filter;
    let current = filter.read().clone();
    let total: usize = counts.iter().sum();
    let tiles: Vec<(String, &'static str, String, Option<Severity>, &'static str)> = Severity::ALL
        .iter()
        .zip(counts)
        .map(|(sev, n)| {
            let none = if n == 0 { " none" } else { "" };
            (
                format!("tile {}{none}", severity_class(*sev)),
                sev.label(),
                n.to_string(),
                Some(*sev),
                pressed(current.severity == Some(*sev)),
            )
        })
        .collect();
    let rule_value = current.rule.unwrap_or("");
    let active = current.is_active();
    rsx! {
        section { class: "tiles", aria_label: "Filter by severity",
            for (class , label , count , value , is_pressed) in tiles {
                button {
                    key: "{label}",
                    class,
                    aria_pressed: is_pressed,
                    title: "Show only {label} findings",
                    onclick: move |_| {
                        let mut f = filter.write();
                        f.severity = if f.severity == value { None } else { value };
                    },
                    span { class: "tile-count", {count} }
                    span { class: "tile-label", {label} }
                }
            }
        }
        section { class: "filters card",
            label { class: "search grow",
                Icon { name: "search", size: 15 }
                span { class: "visually-hidden", "Search findings" }
                input {
                    r#type: "search",
                    placeholder,
                    value: current.text.clone(),
                    oninput: move |e| filter.write().text = e.value(),
                }
            }
            if !databases.is_empty() {
                div { class: "seg", role: "group", aria_label: "Database",
                    for (key , label , value) in databases {
                        button {
                            key: "{key}",
                            aria_pressed: pressed(current.side == value),
                            onclick: move |_| filter.write().side = value,
                            {label}
                        }
                    }
                }
            }
            label { class: "select-field",
                span { class: "visually-hidden", "Rule type" }
                select {
                    onchange: move |e| {
                        let v = e.value();
                        let rule = rules_lookup(&v);
                        filter.write().rule = rule;
                    },
                    option { value: "", selected: rule_value.is_empty(), "All rule types ({total})" }
                    for (rule , n) in rules {
                        option { key: "{rule}", value: "{rule}", selected: rule == rule_value, "{rule} ({n})" }
                    }
                }
            }
            if active {
                button {
                    class: "btn ghost small",
                    onclick: move |_| filter.set(FindingFilter::default()),
                    Icon { name: "x", size: 14 }
                    "Clear filters"
                }
            }
        }
    }
}

/// Finds the rule id with this text among all known ids (they are
/// `&'static str`, so the filter can hold one).
fn rules_lookup(text: &str) -> Option<&'static str> {
    schema_bridge_core::checks::RULES
        .iter()
        .chain(schema_bridge_core::integration::INTEGRATION_RULES)
        .find(|r| **r == text)
        .copied()
}

#[component]
pub fn Problems(
    workspace: Signal<Workspace>,
    filter: Signal<FindingFilter>,
    go_to_load: EventHandler<()>,
) -> Element {
    let d = derived();
    let mut shown = use_signal(|| PAGE);
    let ws = workspace.read();
    let loaded = ws.source.schema.is_some() || ws.target.schema.is_some();
    let name_of = |side: Side| ws.slot(side).name.clone();
    let all = d.findings.read();
    let current = filter.read().clone();

    let head = rsx! {
        section { class: "page-head",
            div {
                h2 { "Schema checks" }
                p { class: "lead",
                    "Findings about the structure of each schema that can affect the integration, read from the structure export alone. They are not errors in the running database: Schema Bridge never connects to it. Each finding says what it means and what to do."
                }
            }
        }
    };

    if !loaded {
        return rsx! {
            {head}
            section { class: "card empty",
                p { class: "empty-title", "No database loaded yet" }
                p { "Load a source or target export first, or try the sample schemas." }
                div {
                    button { class: "btn primary", onclick: move |_| go_to_load.call(()), "Go to Load schemas" }
                }
            }
        };
    }

    // Tiles and rule list count within the chosen database.
    let in_database: Vec<&Finding> = all
        .iter()
        .filter(|(s, _)| current.side.is_none_or(|c| c == *s))
        .map(|(_, f)| f)
        .collect();
    let counts = count_by_severity(in_database.iter().copied());
    let rules = rule_counts(in_database.iter().copied());
    let mut databases = vec![("all".to_string(), format!("All · {}", all.len()), None)];
    for side in [Side::Source, Side::Target] {
        if ws.slot(side).schema.is_some() {
            let n = all.iter().filter(|(s, _)| *s == side).count();
            databases.push((
                side.id().to_string(),
                format!("{} · {n}", name_of(side)),
                Some(side),
            ));
        }
    }

    let matching: Vec<Card> = all
        .iter()
        .enumerate()
        .filter(|(_, (side, f))| current.matches(*side, f))
        .map(|(i, (side, f))| {
            let class = match side {
                Side::Source => "finding-db src",
                Side::Target => "finding-db tgt",
            };
            card(i, f, name_of(*side), class)
        })
        .collect();
    let total = matching.len();
    let limit = shown();
    let cards: Vec<Card> = matching.into_iter().take(limit).collect();
    let status = if total == all.len() {
        format!("Showing all {total} findings")
    } else {
        format!("Showing {total} of {} findings", all.len())
    };
    let more = total.saturating_sub(limit);
    drop(all);
    drop(ws);

    rsx! {
        {head}
        FindingFilters {
            filter,
            counts,
            rules,
            databases,
            placeholder: "Search titles, tables, columns, rules…",
        }
        p { class: "muted shown", role: "status", {status} }
        if cards.is_empty() {
            section { class: "card empty",
                p { class: "empty-title", "No findings match these filters" }
                div {
                    button { class: "btn secondary", onclick: move |_| filter.set(FindingFilter::default()), "Clear filters" }
                }
            }
        } else {
            FindingCards { cards }
            if more > 0 {
                button { class: "btn secondary more", onclick: move |_| shown.set(limit + PAGE), {more_label(more, PAGE)} }
            }
        }
    }
}
