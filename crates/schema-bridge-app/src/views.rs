//! The "Load" stage: what Schema Bridge does, the source and target database
//! panels joined by the bridge, the analysis at a glance and the next step.
//! Values are prepared in plain Rust first, so the markup only places them.

use dioxus::html::{FileData, HasFileData};
use dioxus::prelude::*;
use schema_bridge_core::ExportKind;
use schema_bridge_core::influx::retention_summary;
use schema_bridge_core::model::ColumnRole;
use schema_bridge_core::report::summary;
use schema_bridge_core::systems::exportable;
use schema_bridge_core::workspace::{Notice, Side, Workspace, grouped, system_name};

use crate::browser::{CopyButton, reveal_how_to, reveal_panels};
use crate::export::OpenWorkspaceButton;
use crate::feedback::{Action, confirm_or_apply};
use crate::icons::Icon;
use crate::state::derived;
use crate::widgets::{SummaryCard, summary_cards};

/// The parts of an export, shown as chips in each panel.
const PARTS: [(ExportKind, &str); 4] = [
    (ExportKind::Columns, "Columns"),
    (ExportKind::Constraints, "Keys & links"),
    (ExportKind::Indexes, "Indexes"),
    (ExportKind::RowCounts, "Row counts"),
];

/// The two made-up sample pairs offered on the first screen: what loads
/// them, their name, the pair of databases and what they show.
const SAMPLES: [(Action, &str, &str, &str); 2] = [
    (
        Action::LoadSamples,
        "Relational sample",
        "SQLite workshop → PostgreSQL catalogue",
        "Schema checks, matching, a load plan and draft SQL",
    ),
    (
        Action::LoadTimeseriesSamples,
        "Time-series sample",
        "InfluxDB plant → PostgreSQL readings",
        "Measurements, tags, fields and retention",
    ),
];

/// The four things Schema Bridge does, shown on the first screen.
const PIPELINE: [(&str, &str, &str); 4] = [
    ("load", "Choose two schemas", "A source and a target export"),
    ("problems", "Analyse", "Schema checks on each database"),
    ("matching", "Match", "Tables and columns, reviewed by you"),
    ("plan", "Plan", "Load order, draft SQL and a report"),
];

#[component]
pub fn LoadStage(
    workspace: Signal<Workspace>,
    open_problems: EventHandler<()>,
    open_matching: EventHandler<()>,
    open_report: EventHandler<()>,
) -> Element {
    let d = derived();
    let ws = workspace.read();
    let loaded = (ws.source.schema.is_some(), ws.target.schema.is_some());
    let any = loaded.0 || loaded.1;
    let both = loaded.0 && loaded.1;
    let using_samples = ws.using_samples;
    let analysis = d.analysis.read();
    let s = any.then(|| summary(&ws, analysis.as_ref()));
    drop(analysis);
    drop(ws);
    let cards = s
        .as_ref()
        .map(|s| summary_cards(s, both))
        .unwrap_or_default();
    let finding_count = d.findings.read().len();
    let next = if both {
        Some((
            "Next: review the schema checks and the matching",
            "Start with the schema checks, then see how the tables were matched. The report sums everything up.",
        ))
    } else if any {
        Some((
            "Next: add the other database",
            "The schema checks of the loaded database are ready. Matching, the plan and the report need both databases.",
        ))
    } else {
        None
    };

    rsx! {
        if !any {
            section { class: "hero",
                div { class: "hero-text",
                    h2 { "Compare two database schemas and plan the integration" }
                    p { class: "lead",
                        "Schema Bridge reads a structure export of each database, checks each schema for integration risks, matches their tables and columns, and plans how to bring one into the other. Everything stays on this computer."
                    }
                    div { class: "start-own",
                        h3 { "Use your own databases" }
                        p {
                            "Export each database's structure with one read-only query, then add the two files to the Source and Target panels below."
                        }
                        div { class: "start-own-actions",
                            button {
                                class: "btn primary lg",
                                onclick: move |_| reveal_panels(),
                                Icon { name: "upload", size: 17 }
                                "Add your exports"
                            }
                            button {
                                class: "btn ghost",
                                onclick: move |_| reveal_how_to(),
                                "How to make an export file"
                            }
                        }
                    }
                    div { class: "start-samples",
                        h3 { "Or explore a sample" }
                        div { class: "sample-cards",
                            for (action , title , pair , shows) in SAMPLES {
                                button {
                                    key: "{title}",
                                    class: "sample-card",
                                    onclick: move |_| confirm_or_apply(workspace, action.clone()),
                                    span { class: "sample-title",
                                        {title}
                                        Icon { name: "arrow", size: 15 }
                                    }
                                    span { class: "sample-pair", {pair} }
                                    span { class: "sample-shows", {shows} }
                                }
                            }
                        }
                    }
                    p { class: "start-resume",
                        "Continuing earlier work?"
                        OpenWorkspaceButton { workspace, id: "open-workspace-hero", class: "btn ghost small" }
                    }
                }
                ol { class: "pipeline", aria_label: "What Schema Bridge does",
                    for (i , (icon , title , text)) in PIPELINE.iter().enumerate() {
                        li { key: "{title}",
                            span { class: "pipeline-icon", Icon { name: icon, size: 18 } }
                            span { class: "pipeline-text",
                                b { "{i + 1}. {title}" }
                                span { {*text} }
                            }
                        }
                    }
                }
            }
        } else {
            section { class: "page-head",
                div {
                    h2 { "Load schemas" }
                    p { class: "lead",
                        "The source is the database you bring in; the target is the one that receives it."
                    }
                }
                div { class: "page-actions",
                    if using_samples {
                        span { class: "badge info", "Sample data" }
                    }
                    button {
                        class: "btn ghost",
                        title: "Make the source the target and the target the source",
                        onclick: move |_| confirm_or_apply(workspace, Action::Swap),
                        "⇄ Swap"
                    }
                    OpenWorkspaceButton { workspace, id: "open-workspace-load", class: "btn ghost" }
                    if !using_samples {
                        button {
                            class: "btn ghost",
                            onclick: move |_| confirm_or_apply(workspace, Action::LoadSamples),
                            "Load sample schemas"
                        }
                        button {
                            class: "btn ghost",
                            title: "InfluxDB measurements into a PostgreSQL database",
                            onclick: move |_| confirm_or_apply(workspace, Action::LoadTimeseriesSamples),
                            "Time-series sample"
                        }
                    }
                }
            }
        }
        div { class: "bench",
            DatabasePanel { workspace, side: Side::Source }
            Bridge { workspace }
            DatabasePanel { workspace, side: Side::Target }
        }
        if !cards.is_empty() {
            section { class: "analysis", aria_label: "The analysis at a glance",
                for card in cards {
                    SummaryCard { key: "{card.title}", data: card }
                }
            }
        }
        if let Some((title, text)) = next {
            section { class: "next-band",
                div {
                    h3 { {title} }
                    p { {text} }
                }
                div { class: "next-actions",
                    if finding_count > 0 {
                        button { class: "btn primary", onclick: move |_| open_problems.call(()),
                            "Review {finding_count} schema findings"
                            Icon { name: "arrow", size: 15 }
                        }
                    }
                    if both {
                        button { class: "btn secondary", onclick: move |_| open_matching.call(()), "Check the matching" }
                        button { class: "btn ghost", onclick: move |_| open_report.call(()), "Open the report" }
                    }
                }
            }
        }
        if both {
            SideBySide { workspace }
        }
        HowToExport {}
    }
}

/// The connection between the two panels: what the source becomes in the
/// target, once both are loaded.
#[component]
fn Bridge(workspace: Signal<Workspace>) -> Element {
    let d = derived();
    let ws = workspace.read();
    let loaded = (ws.source.schema.is_some(), ws.target.schema.is_some());
    let noun = ws
        .source
        .schema
        .as_ref()
        .map_or("tables", |x| x.object_noun());
    drop(ws);
    let analysis = d.analysis.read();
    let (state, title, text) = match (loaded, analysis.as_ref()) {
        (_, Some(a)) => {
            let m = &a.matching;
            (
                "bridge linked",
                format!("{} of {} {noun}", m.mappings.len(), m.tables.len()),
                format!("matched · {} integration issues", a.integration.len()),
            )
        }
        ((true, false), _) => (
            "bridge half",
            "Waiting".to_string(),
            "for the target".to_string(),
        ),
        ((false, true), _) => (
            "bridge half",
            "Waiting".to_string(),
            "for the source".to_string(),
        ),
        _ => ("bridge", "goes into".to_string(), String::new()),
    };
    rsx! {
        div { class: state, role: "presentation",
            span { class: "bridge-line", aria_hidden: "true" }
            span { class: "bridge-node", aria_hidden: "true", Icon { name: "bridge", size: 22 } }
            span { class: "bridge-label",
                b { {title} }
                if !text.is_empty() {
                    span { {text} }
                }
            }
        }
    }
}

/// How to make an export file: one read-only query per database system and
/// the steps to run it (the prototype's "How to export" tab).
#[component]
fn HowToExport() -> Element {
    let mut chosen = use_signal(|| 0usize);
    let systems: Vec<_> = exportable().collect();
    let (system, current) = systems[chosen().min(systems.len() - 1)];
    let tested = if current.tested {
        "Tested on a real database."
    } else {
        "Reviewed, but not yet run on a real server. Please report problems."
    };
    rsx! {
        details { class: "card how-to", id: "how-to-export",
            summary {
                Icon { name: "info", size: 16 }
                "How to make an export file"
            }
            div { class: "how-to-body",
                p { class: "muted",
                    "Run one read-only query in your usual database tool and save the result as CSV. It reads the structure only (tables, columns, keys, indexes), never table contents, and changes nothing. Schema Bridge never connects to the database itself."
                }
                div { class: "seg", role: "group", aria_label: "Database system",
                    for (i , (s , _)) in systems.iter().enumerate() {
                        button {
                            key: "{s.code}",
                            aria_pressed: if i == chosen() { "true" } else { "false" },
                            onclick: move |_| chosen.set(i),
                            {s.name}
                        }
                    }
                }
                ol { class: "how-to-steps",
                    for step in current.steps {
                        li { key: "{step}", {step} }
                    }
                    li { "Add the CSV file to the Source or Target panel above. Repeat for the other database." }
                }
                div { class: "how-to-query-head",
                    span { class: "muted", "Query for {system.name} · {tested}" }
                    CopyButton { text: current.query, label: "Copy query" }
                }
                pre { class: "sql", tabindex: "0", {current.query} }
            }
        }
    }
}

/// One table row in a panel's table list.
struct TableRow {
    key: String,
    name: String,
    columns: String,
}

/// What the panel's error card says is accepted, from the systems that have
/// an export query.
fn accepted_formats<'a>(systems: impl IntoIterator<Item = &'a str>) -> String {
    let mut names: Vec<&str> = systems.into_iter().collect();
    let last = names.pop().unwrap_or_default();
    let list = if names.is_empty() {
        last.to_string()
    } else {
        format!("{} or {last}", names.join(", "))
    };
    format!(
        "Accepted: the CSV file made with the export query for {list}, or the older four-file PostgreSQL export."
    )
}

/// A short explanation of each panel figure, shown on hover.
fn stat_hint(label: &str) -> &'static str {
    match label {
        "Tables" => "Tables in the export (views are not counted)",
        "Columns" => "Columns in all tables together",
        "Links" => "Foreign keys: columns that point to a row in another table",
        "Rows (est.)" => "The database's own row estimates added up; they may be out of date",
        "Measurements" => "Measurements in the exported bucket",
        "Tags" => "Tag keys in all measurements together",
        "Fields" => "Field keys in all measurements together",
        "Series" => "Tag combinations in the export, not a number of points",
        _ => "",
    }
}

/// Reads files into one database, one after the other. `reading` names the
/// file being read, so the panel can say so and refuse other files.
async fn load_files(
    mut workspace: Signal<Workspace>,
    side: Side,
    files: Vec<FileData>,
    mut reading: Signal<Option<String>>,
) {
    for file in files {
        let file_name = file.name();
        reading.set(Some(file_name.clone()));
        match file.read_string().await {
            Ok(text) => workspace.write().add_file(side, &file_name, text),
            Err(_) => workspace
                .write()
                .slot_mut(side)
                .reject_file(&file_name, "The file could not be read."),
        }
    }
    reading.set(None);
}

#[component]
fn DatabasePanel(workspace: Signal<Workspace>, side: Side) -> Element {
    let mut workspace = workspace;
    // The file being read, if any.
    let reading = use_signal(|| None::<String>);
    // Drag events entered minus left: above zero while files are over the
    // panel (moving over child elements fires both events).
    let mut drag = use_signal(|| 0i32);
    let slot = workspace.read().slot(side).clone();
    let busy = reading().is_some();

    let role = side.label();
    let role_hint = match side {
        Side::Source => "the database you bring in",
        Side::Target => "the database that receives it",
    };
    let name = slot.name.clone();
    let name_label = format!("{role} database name");
    let input_id = format!("file-{}", side.id());
    let loaded = slot.schema.is_some();
    let panel_class = format!(
        "dbpanel dbpanel-{} {}{}{}",
        side.id(),
        if loaded { "is-loaded" } else { "is-empty" },
        if drag() > 0 { " is-dragging" } else { "" },
        if busy { " is-reading" } else { "" }
    );

    let chips: Vec<(&str, &str)> = PARTS
        .iter()
        .map(|(kind, label)| {
            (
                *label,
                if slot.input.csv_files().is_some_and(|f| f.has(*kind)) {
                    "chip on"
                } else {
                    "chip"
                },
            )
        })
        .collect();

    let file = match &slot.notice {
        Some(Notice::Loaded { file, part }) => Some(format!("{file} · {part}")),
        _ => None,
    };
    let error = match &slot.notice {
        Some(Notice::Rejected { file, reason }) => Some((file.clone(), reason.clone())),
        _ => None,
    };
    let accepted = accepted_formats(exportable().map(|(s, _)| s.name));
    let time_series = slot.schema.as_ref().is_some_and(|x| x.is_time_series());
    let retention = slot.schema.as_ref().and_then(retention_summary);
    let noun = slot.schema.as_ref().map_or("tables", |x| x.object_noun());
    let summary = slot.schema.as_ref().map(|schema| {
        let stats = if time_series {
            let s = schema.time_series_stats();
            vec![
                ("Measurements", grouped(s.measurements as i64)),
                ("Tags", grouped(s.tags as i64)),
                ("Fields", grouped(s.fields as i64)),
                ("Series", s.series.map_or("–".to_string(), grouped)),
            ]
        } else {
            let s = schema.stats();
            vec![
                ("Tables", grouped(s.tables as i64)),
                ("Columns", grouped(s.columns as i64)),
                ("Links", grouped(s.links as i64)),
                ("Rows (est.)", grouped(s.estimated_rows)),
            ]
        };
        let tables: Vec<TableRow> = schema
            .tables
            .iter()
            .map(|t| {
                let count = |role| t.columns.iter().filter(|c| c.role == role).count();
                TableRow {
                    key: t.key.clone(),
                    name: t.name.clone(),
                    columns: if time_series {
                        format!(
                            "{} tags · {} fields",
                            count(ColumnRole::Tag),
                            count(ColumnRole::Field)
                        )
                    } else {
                        format!("{} columns", t.columns.len())
                    },
                }
            })
            .collect();
        (system_name(&schema.dbms).to_string(), stats, tables)
    });
    let (state_class, state_text) = if busy {
        ("state reading", "Reading…")
    } else if loaded {
        ("state loaded", "Loaded")
    } else {
        ("state empty", "No export yet")
    };
    let role_lower = role.to_lowercase();
    let add_label = if loaded {
        "Replace export"
    } else {
        "Add export file"
    };

    rsx! {
        article {
            class: panel_class,
            aria_label: "{role} database",
            aria_busy: if busy { "true" } else { "false" },
            ondragenter: move |e| {
                e.prevent_default();
                drag += 1;
            },
            ondragover: move |e| e.prevent_default(),
            ondragleave: move |_| {
                let n = drag();
                drag.set((n - 1).max(0));
            },
            ondrop: move |e| async move {
                e.prevent_default();
                drag.set(0);
                let files = e.files();
                if reading().is_none() && !files.is_empty() {
                    load_files(workspace, side, files, reading).await;
                }
            },
            if drag() > 0 && !busy {
                div { class: "drop-overlay", aria_hidden: "true",
                    Icon { name: "upload", size: 22 }
                    b { if loaded { "Drop to replace the {role_lower} export" } else { "Drop to add the {role_lower} export" } }
                }
            }
            header { class: "dbpanel-head",
                div { class: "dbpanel-role",
                    span { class: "role-mark", aria_hidden: "true", Icon { name: "load", size: 16 } }
                    span { class: "role-text",
                        b { {role} }
                        span { {role_hint} }
                    }
                    span { class: state_class, role: "status",
                        span { class: "state-dot", aria_hidden: "true" }
                        {state_text}
                    }
                }
                input {
                    class: "dbpanel-name",
                    r#type: "text",
                    value: name,
                    aria_label: name_label,
                    title: "Click to rename",
                    oninput: move |e| workspace.write().slot_mut(side).name = e.value(),
                }
                if let Some((system , _ , _)) = &summary {
                    p { class: "dbpanel-engine",
                        span { class: "engine", {system.clone()} }
                        if time_series {
                            span { class: "badge ts", "Time series" }
                        }
                        if let Some(file) = file.clone() {
                            span { class: "file", {file} }
                        }
                    }
                }
            }
            if let Some((_ , stats , tables)) = summary {
                dl { class: "dbstats",
                    for (label , value) in stats {
                        div { key: "{label}", title: stat_hint(label),
                            dt { {label} }
                            dd { {value} }
                        }
                    }
                }
                if let Some(retention) = retention {
                    p { class: "retention",
                        b { "Retention " }
                        {retention}
                    }
                }
                if slot.input.csv_files().is_some() {
                    div { class: "chips",
                        for (label , class) in chips {
                            span { key: "{label}", class, {label} }
                        }
                    }
                }
                details { class: "table-list",
                    summary { "Show the {tables.len()} {noun}" }
                    ul {
                        for TableRow { key , name , columns } in tables {
                            li { key: "{key}",
                                span { class: "mono", {name} }
                                span { class: "muted", {columns} }
                            }
                        }
                    }
                }
            } else {
                label { class: "dropzone", r#for: input_id.clone(),
                    span { class: "dropzone-icon", Icon { name: "upload", size: 22 } }
                    b { "Add the structure export" }
                    span { "Drop the CSV file made with the export query here, or click to choose it." }
                }
            }
            if let Some(file) = reading() {
                p { class: "reading-line", role: "status",
                    span { class: "state-dot", aria_hidden: "true" }
                    "Reading {file}…"
                }
            }
            if let Some((file, reason)) = error {
                div { class: "load-error", role: "alert",
                    div { class: "load-error-head",
                        b { "{file} could not be loaded" }
                        button {
                            class: "load-error-close",
                            aria_label: "Dismiss this message",
                            title: "Dismiss",
                            onclick: move |_| workspace.write().slot_mut(side).notice = None,
                            "×"
                        }
                    }
                    p { {reason} }
                    p { class: "load-error-help", {accepted} }
                    if loaded {
                        p { class: "load-error-help", "What was loaded before is unchanged." }
                    }
                    div {
                        button {
                            class: "btn secondary small",
                            onclick: move |_| reveal_how_to(),
                            "Show me how to export"
                        }
                    }
                }
            }
            div { class: "dbpanel-actions",
                label {
                    class: if loaded { "btn secondary" } else { "btn primary" },
                    r#for: input_id.clone(),
                    aria_disabled: if busy { "true" } else { "false" },
                    Icon { name: "upload", size: 15 }
                    {add_label}
                }
                input {
                    id: input_id,
                    class: "visually-hidden",
                    r#type: "file",
                    accept: ".csv,text/csv",
                    multiple: true,
                    disabled: busy,
                    onchange: move |evt| async move {
                        if reading().is_none() {
                            load_files(workspace, side, evt.files(), reading).await;
                        }
                    },
                }
                if loaded {
                    button {
                        class: "btn ghost",
                        disabled: busy,
                        onclick: move |_| confirm_or_apply(workspace, Action::Clear(side)),
                        "Clear"
                    }
                }
            }
        }
    }
}

/// The two databases' structure side by side, folded away by default.
#[component]
fn SideBySide(workspace: Signal<Workspace>) -> Element {
    let ws = workspace.read();
    let (Some(a), Some(b)) = (&ws.source.schema, &ws.target.schema) else {
        return rsx! {};
    };
    let (x, y) = (a.stats(), b.stats());
    let rows: Vec<(&str, String, String)> = [
        ("Tables", x.tables as i64, y.tables as i64),
        ("Columns", x.columns as i64, y.columns as i64),
        ("Links (foreign keys)", x.links as i64, y.links as i64),
        (
            "Tables with a primary key",
            x.tables_with_primary_key as i64,
            y.tables_with_primary_key as i64,
        ),
        (
            "Columns with a description",
            x.columns_with_description as i64,
            y.columns_with_description as i64,
        ),
        ("Rows (estimated)", x.estimated_rows, y.estimated_rows),
    ]
    .into_iter()
    .map(|(label, left, right)| (label, grouped(left), grouped(right)))
    .collect();
    let source_name = ws.source.name.clone();
    let target_name = ws.target.name.clone();

    rsx! {
        details { class: "card overview",
            summary { "Compare the structure side by side" }
            table { class: "facts",
                thead {
                    tr {
                        th { scope: "col",
                            span { class: "visually-hidden", "Measure" }
                        }
                        th { scope: "col", class: "num src", {source_name} }
                        th { scope: "col", class: "num tgt", {target_name} }
                    }
                }
                tbody {
                    for (label , left , right) in rows {
                        tr { key: "{label}",
                            th { scope: "row", {label} }
                            td { class: "num", {left} }
                            td { class: "num", {right} }
                        }
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
    fn the_error_card_lists_every_system_with_an_export_query() {
        let text = accepted_formats(exportable().map(|(s, _)| s.name));
        for (system, _) in exportable() {
            assert!(
                text.contains(system.name),
                "{} missing: {text}",
                system.name
            );
        }
        assert!(text.contains("four-file PostgreSQL export"));
        assert_eq!(
            accepted_formats(["A", "B", "C"]),
            "Accepted: the CSV file made with the export query for A, B or C, or the older four-file PostgreSQL export."
        );
        assert!(accepted_formats(["A"]).contains("query for A, or"));
    }

    #[test]
    fn every_panel_figure_is_explained() {
        for label in [
            "Tables",
            "Columns",
            "Links",
            "Rows (est.)",
            "Measurements",
            "Tags",
            "Fields",
            "Series",
        ] {
            assert!(!stat_hint(label).is_empty(), "{label}");
        }
    }

    #[test]
    fn both_samples_are_offered() {
        let actions: Vec<&Action> = SAMPLES.iter().map(|(a, ..)| a).collect();
        assert_eq!(
            actions,
            [&Action::LoadSamples, &Action::LoadTimeseriesSamples]
        );
    }
}
