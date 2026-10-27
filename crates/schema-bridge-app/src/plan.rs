//! The "Plan" and "Draft SQL" stages: the six steps of the integration plan
//! with the load order, and the draft SQL for a test load. Follows the
//! prototype's "Integration plan" tab. Everything is computed by the core
//! library; the SQL is shown and can be copied or saved, never run.

use dioxus::prelude::*;
use schema_bridge_core::report::plan_steps;
use schema_bridge_core::workspace::Workspace;
use schema_bridge_core::{Finding, Severity, table_name};

use crate::browser::{CopyButton, download_text, file_stem};
use crate::problems::severity_class;
use crate::state::derived;

/// One line in a step's list: severity pill, title, object.
#[derive(Debug, Clone, PartialEq)]
struct Item {
    pill_class: String,
    severity: &'static str,
    title: String,
    object: String,
}

fn items<'a>(findings: impl IntoIterator<Item = &'a Finding>) -> Vec<Item> {
    findings
        .into_iter()
        .filter(|f| f.severity <= Severity::High)
        .map(|f| Item {
            pill_class: format!("pill {}", severity_class(f.severity)),
            severity: f.severity.label(),
            title: f.title.clone(),
            object: f.object.clone(),
        })
        .collect()
}

#[component]
fn ItemList(items: Vec<Item>, empty: String) -> Element {
    if items.is_empty() {
        return rsx! {
            p { class: "plan-ok", {empty} }
        };
    }
    rsx! {
        ul { class: "plan-items",
            for (i , Item { pill_class , severity , title , object }) in items.into_iter().enumerate() {
                li { key: "{i}",
                    span { class: pill_class, {severity} }
                    span { class: "plan-item-text",
                        span { class: "plan-item-title", {title} }
                        if !object.is_empty() {
                            span { class: "plan-item-object mono", {object} }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Step(number: usize, title: String, hint: String, children: Element) -> Element {
    rsx! {
        section { class: "card plan-step",
            span { class: "plan-num", aria_hidden: "true", {number.to_string()} }
            div { class: "plan-body",
                h3 { {title} }
                p { class: "muted plan-hint", {hint} }
                {children}
            }
        }
    }
}

fn needs_both(go_to_load: EventHandler<()>, what: &str) -> Element {
    let text = format!(
        "{what} needs both databases. Load the source and the target export first, or try the sample schemas."
    );
    rsx! {
        section { class: "card empty",
            p { {text} }
            div {
                button { class: "btn primary", onclick: move |_| go_to_load.call(()), "Go to Load schemas" }
            }
        }
    }
}

/// The model decisions a time-series comparison leaves to people: Schema
/// Bridge shows the differences, but does not choose the target design.
/// `None` when neither database is a time-series schema.
fn time_series_design(
    source_ts: bool,
    target_ts: bool,
) -> Option<(&'static str, Vec<&'static str>)> {
    match (source_ts, target_ts) {
        (true, false) => Some((
            "Schema Bridge shows the measurements, tags, fields, time, retention, mappings and type differences, but it does not choose how the time-series data is modelled in the relational target. Decide, with the people who know the data:",
            vec![
                "The target tables: one per measurement, one shared table, or another model.",
                "Tags and fields: which become columns, keys or lookup tables.",
                "Time: the column type and precision (nanoseconds are cut to microseconds or less) and the time zone.",
                "Indexes and unique rules, for example on tags plus time.",
                "Storage, retention and performance: how much history to keep, partitioning, compression.",
                "Whether a time-series extension fits, for example TimescaleDB on PostgreSQL. It is one option, not a requirement.",
            ],
        )),
        (_, true) => Some((
            "Schema Bridge shows how the columns line up with the measurement, but it does not choose the time-series design. Decide, with the people who know the data:",
            vec![
                "Which columns become tags (they identify a series) and which become fields.",
                "Which column holds the time, and its precision.",
                "The measurements and the bucket's retention.",
                "How the data is written: Schema Bridge generates no SQL and no line protocol for a time-series target.",
            ],
        )),
        _ => None,
    }
}

#[component]
pub fn PlanStage(
    workspace: Signal<Workspace>,
    go_to_load: EventHandler<()>,
    go_to_sql: EventHandler<()>,
) -> Element {
    let ws = workspace.read();
    let (s, t) = (ws.source.name.clone(), ws.target.name.clone());
    let head = rsx! {
        section { class: "page-head",
            div {
                h2 { "Plan" }
                p { class: "lead",
                    "The integration as six steps, in the order they should happen. Work through them with the people who know both databases."
                }
            }
        }
    };
    let d = derived();
    let analysis = d.analysis.read();
    let Some(analysis) = analysis.as_ref() else {
        drop(ws);
        return rsx! {
            {head}
            {needs_both(go_to_load, "The plan")}
        };
    };
    let steps = plan_steps(&ws, analysis);
    let sql_available = ws.sql_available();
    let is_ts =
        |x: &Option<schema_bridge_core::Schema>| x.as_ref().is_some_and(|x| x.is_time_series());
    let design = time_series_design(is_ts(&ws.source.schema), is_ts(&ws.target.schema));
    drop(ws);
    let fix_target = items(&steps.fix_target);
    let check_source = items(&steps.check_source);
    let mut decisions = items(&steps.decisions);
    if !steps.no_place.is_empty() {
        decisions.push(Item {
            pill_class: format!("pill {}", severity_class(Severity::Medium)),
            severity: Severity::Medium.label(),
            title: format!(
                "No place yet for {} table{}",
                steps.no_place.len(),
                if steps.no_place.len() == 1 { "" } else { "s" }
            ),
            object: steps.no_place.join(", "),
        });
    }

    // Load order strip.
    let order: Vec<(String, String, String, String)> = steps
        .load_order
        .iter()
        .map(|(table, from, cycle)| {
            let class = match (cycle, from.is_empty()) {
                (true, _) => "order-chip cycle",
                (false, true) => "order-chip existing",
                (false, false) => "order-chip load",
            };
            let shown_from = if from.is_empty() {
                String::new()
            } else {
                format!("← {}", from.join(", "))
            };
            let title = if from.is_empty() {
                format!("Not loaded from {s}: must already contain the rows other tables link to")
            } else {
                format!("Loaded from {}", from.join(", "))
            };
            (class.to_string(), table.clone(), shown_from, title)
        })
        .collect();
    let cycle: Vec<&str> = steps
        .load_order
        .iter()
        .filter(|(_, _, c)| *c)
        .map(|(t, _, _)| t.as_str())
        .collect();
    let cycle_note = (!cycle.is_empty()).then(|| {
        format!(
            "These tables link to each other in a circle, so no order works on its own: {}. Load them in two passes: insert the rows with the circular link empty, then fill it with an update.",
            cycle.join(", ")
        )
    });
    let todos = steps.todos;
    let implicit = steps.unchecked_links.clone();

    let todo_text = match todos {
        0 => "The draft has no open TODOs.".to_string(),
        1 => "The draft has 1 TODO to resolve first.".to_string(),
        n => format!("The draft has {n} TODOs to resolve first."),
    };

    rsx! {
        {head}
        div { class: "plan",
            Step {
                number: 1,
                title: format!("Fix the schema findings in {t}"),
                hint: "The receiving database must be correct first, otherwise good data is rejected or stored wrongly.",
                ItemList { items: fix_target, empty: "Nothing serious found." }
            }
            Step {
                number: 2,
                title: format!("Check the data in {s}"),
                hint: "These rules are missing in the source, so bad rows may exist. Count them before you copy anything.",
                ItemList { items: check_source, empty: "Nothing serious found." }
            }
            Step {
                number: 3,
                title: "Make the decisions",
                hint: "Schema Bridge cannot decide these; they need the people who know the data.",
                ItemList { items: decisions, empty: "No open decisions." }
                if let Some((lead, points)) = design {
                    div { class: "callout", role: "note",
                        b { "Design decisions that stay with you. " }
                        {lead}
                        ul {
                            for p in points {
                                li { key: "{p}", {p} }
                            }
                        }
                    }
                }
            }
            Step {
                number: 4,
                title: "Load the tables in this order",
                hint: format!(
                    "A table can only be loaded after the tables it links to. Outlined tables are not loaded from {s}; they must already contain the rows you need.",
                ),
                if order.is_empty() {
                    p { class: "plan-ok", "No table goes into {t} yet. Match tables on the Matching stage first." }
                } else {
                    ol { class: "order",
                        for (i , (class , name , from , title)) in order.into_iter().enumerate() {
                            li { key: "{name}", class, title,
                                span { class: "order-pos", {(i + 1).to_string()} }
                                span { class: "mono", {name} }
                                if !from.is_empty() {
                                    span { class: "order-from mono", {from} }
                                }
                            }
                        }
                    }
                }
                if let Some(note) = cycle_note {
                    p { class: "callout warning", role: "note", {note} }
                }
            }
            if sql_available {
                Step {
                    number: 5,
                    title: "Run a test load with the draft SQL",
                    hint: "Every TODO marks a decision from step 3. Run it on a test copy first, inside a transaction you can roll back.",
                    div { class: "plan-row",
                        p { {todo_text} }
                        button { class: "btn primary", onclick: move |_| go_to_sql.call(()), "Open draft SQL →" }
                    }
                }
            } else {
                Step {
                    number: 5,
                    title: "Run a test load with the target's own tools",
                    hint: "Schema Bridge drafts SQL only for relational targets.",
                    p { class: "callout", role: "note",
                        "{t} is a time-series database. Time-series databases are not written with SQL INSERT statements, and Schema Bridge does not generate InfluxDB line protocol or other write operations. Use the mapping (Matching stage, or the mapping CSV on Export) with the database's own tools."
                    }
                }
            }
            Step {
                number: 6,
                title: "Check the result",
                hint: "Before anyone relies on the loaded data.",
                ul { class: "plan-checks",
                    li { "Row counts: every source table and its target table should have the same number of rows (minus rows you filtered on purpose)." }
                    li { "Links: every copied link points to a real record in {t}." }
                    li { "Run the import a second time. Nothing should change; if duplicates appear, the source ID is not stored or not checked." }
                    if !implicit.is_empty() {
                        li {
                            "Unchecked links in {s} to count before loading: "
                            span { class: "mono", {implicit.join(", ")} }
                        }
                    }
                    li { "Open three or four records in both databases and compare them by eye." }
                }
            }
        }
    }
}

/// Classes for one line of SQL: comments are quieter, TODOs stand out.
fn line_class(line: &str) -> &'static str {
    let todo = line.contains("TODO");
    let comment = line.trim_start().starts_with("--");
    match (comment, todo) {
        (_, true) => "sql-line todo",
        (true, false) => "sql-line comment",
        _ => "sql-line",
    }
}

#[component]
pub fn SqlStage(
    workspace: Signal<Workspace>,
    scope: Signal<Option<String>>,
    go_to_load: EventHandler<()>,
    go_to_plan: EventHandler<()>,
) -> Element {
    let mut scope = scope;
    let ws = workspace.read();
    let (s, t) = (ws.source.name.clone(), ws.target.name.clone());
    let head = rsx! {
        section { class: "page-head",
            div {
                h2 { "Draft SQL" }
                p { class: "lead",
                    "A draft for a test load, written in PostgreSQL syntax, to review with someone who knows both databases. Schema Bridge never runs it and never connects to a database."
                }
            }
            div { class: "page-actions",
                button { class: "btn ghost", onclick: move |_| go_to_plan.call(()), "← Back to the plan" }
            }
        }
    };
    let d = derived();
    let analysis = d.analysis.read();
    let Some(analysis) = analysis.as_ref() else {
        drop(ws);
        return rsx! {
            {head}
            {needs_both(go_to_load, "Draft SQL")}
        };
    };

    // What to show: the whole script or one table.
    let chosen = scope().filter(|k| analysis.matching.mapping(k).is_some());
    let sql = match chosen.as_deref().and_then(|k| analysis.matching.mapping(k)) {
        Some(p) => ws.table_sql(p),
        None => ws.draft_sql(analysis),
    };
    let mut options = vec![(
        String::new(),
        "Whole script (all tables, in load order)".to_string(),
        chosen.is_none(),
    )];
    options.extend(analysis.matching.mappings.iter().map(|p| {
        (
            p.source.clone(),
            format!("{} → {}", table_name(&p.source), table_name(&p.target)),
            chosen.as_deref() == Some(p.source.as_str()),
        )
    }));
    let todos = sql.matches("TODO").count();
    let sql_available = ws.sql_available();
    let not_postgres = ws
        .target
        .schema
        .as_ref()
        .is_some_and(|x| x.dbms != "postgresql");
    let file_name = match chosen.as_deref() {
        Some(k) => format!("{}-{}-draft.sql", file_stem(&s), table_name(k)),
        None => format!("{}-into-{}-draft.sql", file_stem(&s), file_stem(&t)),
    };
    drop(ws);

    let lines: Vec<(&'static str, String)> = sql
        .lines()
        .map(|l| (line_class(l), l.to_string()))
        .collect();
    let todo_text = match todos {
        0 => "No TODOs in this draft.".to_string(),
        1 => "1 TODO marks a decision that is still open.".to_string(),
        n => format!("{n} TODOs mark decisions that are still open."),
    };
    let download = sql.clone();

    if !sql_available {
        return rsx! {
            {head}
            div { class: "callout", role: "note",
                b { "Not available for a time-series target. " }
                "{t} is a time-series database. Time-series databases are not written with SQL INSERT statements, and Schema Bridge does not generate InfluxDB line protocol or other write operations. Use the mapping on the Matching stage, or save it as CSV on the Export stage, and write the data with the database's own tools."
            }
        };
    }

    rsx! {
        {head}
        div { class: "callout warning", role: "note",
            b { "Review before running. " }
            "Run it only on a test copy of {t}, inside the transaction it opens; it ends without COMMIT on purpose. {todo_text}"
            if not_postgres {
                " {t} is not PostgreSQL, so use the draft as a column-by-column guide and adapt the statements."
            }
        }
        section { class: "card flush sql-card",
            div { class: "sql-toolbar",
                label { class: "sql-scope",
                    span { class: "visually-hidden", "Show" }
                    select {
                        onchange: move |e| {
                            let v = e.value();
                            scope.set(if v.is_empty() { None } else { Some(v) });
                        },
                        for (value , label , sel) in options {
                            option { key: "{value}", value: "{value}", selected: sel, {label} }
                        }
                    }
                }
                div { class: "sql-actions",
                    CopyButton { text: sql.clone(), label: "Copy SQL" }
                    button {
                        class: "btn secondary",
                        onclick: move |_| download_text(&file_name, "application/sql", download.clone()),
                        "Save as .sql"
                    }
                }
            }
            pre { class: "sql", tabindex: "0", aria_label: "Draft SQL",
                code {
                    for (i , (class , line)) in lines.into_iter().enumerate() {
                        span { key: "{i}", class, "{line}\n" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_series_comparisons_name_the_design_decisions() {
        assert!(time_series_design(false, false).is_none());
        let (lead, points) = time_series_design(true, false).unwrap();
        assert!(lead.contains("does not choose"));
        for topic in [
            "target tables",
            "Tags and fields",
            "Time:",
            "Indexes",
            "compression",
            "TimescaleDB",
        ] {
            assert!(points.iter().any(|p| p.contains(topic)), "{topic}");
        }
        assert!(points.iter().any(|p| p.contains("not a requirement")));
        let (_, points) = time_series_design(false, true).unwrap();
        assert!(points.iter().any(|p| p.contains("tags")));
    }
}
