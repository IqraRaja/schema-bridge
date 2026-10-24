//! The "Export" stage: the report, the findings and the mapping for a
//! meeting or a ticket, the draft SQL, and saving or opening the whole
//! workspace. The texts are produced by the core library (`export`,
//! `workspace_file`); files are created in the browser and never uploaded.

use dioxus::prelude::*;
use schema_bridge_core::export::{all_findings, findings_csv, findings_table, mapping_csv};
use schema_bridge_core::report::{report_html, report_markdown};
use schema_bridge_core::workspace::{Side, Workspace};
use schema_bridge_core::workspace_file::{open_workspace, save_workspace};

use crate::browser::{CopyButton, download_text, file_stem};
use crate::icons::Icon;
use crate::state::derived;

/// "Open workspace…": reads a saved workspace file and replaces the current
/// workspace. Says why when the file is not a workspace.
#[component]
pub fn OpenWorkspaceButton(workspace: Signal<Workspace>, id: String, class: String) -> Element {
    let mut workspace = workspace;
    let mut error = use_signal(|| None::<String>);
    let message = error();
    rsx! {
        span { class: "open-workspace",
            label { class, r#for: id.clone(), "Open workspace…" }
            input {
                id,
                class: "visually-hidden",
                r#type: "file",
                accept: ".json,application/json",
                onchange: move |evt| async move {
                    let Some(file) = evt.files().into_iter().next() else {
                        return;
                    };
                    let name = file.name();
                    match file.read_string().await {
                        Ok(text) => match open_workspace(&text) {
                            Ok(opened) => {
                                *workspace.write() = opened;
                                error.set(None);
                            }
                            Err(e) => error.set(Some(format!("{name}: {e}"))),
                        },
                        Err(_) => error.set(Some(format!("{name}: the file could not be read."))),
                    }
                },
            }
            if let Some(message) = message {
                span { class: "notice error inline", role: "alert", {message} }
            }
        }
    }
}

#[component]
fn ExportCard(title: String, text: String, children: Element) -> Element {
    rsx! {
        section { class: "card export-card",
            h3 { {title} }
            p { class: "muted", {text} }
            div { class: "export-actions", {children} }
        }
    }
}

#[component]
pub fn ExportStage(
    workspace: Signal<Workspace>,
    go_to_load: EventHandler<()>,
    go_to_sql: EventHandler<()>,
) -> Element {
    let d = derived();
    let ws = workspace.read();
    let loaded = ws.source.schema.is_some() || ws.target.schema.is_some();
    let analysis = d.analysis.read();
    let (s, t) = (ws.source.name.clone(), ws.target.name.clone());
    let stem = if analysis.is_some() {
        format!("{}-into-{}", file_stem(&s), file_stem(&t))
    } else if ws.source.schema.is_some() {
        file_stem(&s)
    } else {
        file_stem(&t)
    };

    let head = rsx! {
        section { class: "page-head",
            div {
                h2 { "Export" }
                p { class: "lead",
                    "Take the results to a meeting or a ticket, or save the comparison to continue later. Files are created on this computer; nothing is uploaded, and no SQL is ever run."
                }
            }
        }
    };
    if !loaded {
        drop(ws);
        drop(analysis);
        return rsx! {
            {head}
            section { class: "card empty",
                p { "Nothing to export yet. Load a source or target export, or open a saved workspace." }
                div { class: "export-actions",
                    button { class: "btn primary", onclick: move |_| go_to_load.call(()), "Go to Load schemas" }
                    OpenWorkspaceButton { workspace, id: "open-workspace-export-empty", class: "btn secondary" }
                }
            }
        };
    }

    let findings = all_findings(&ws, analysis.as_ref());
    let count = |side: Option<Side>| findings.iter().filter(|(s, _)| *s == side).count();
    let mut parts = vec![];
    for side in [Side::Source, Side::Target] {
        if ws.slot(side).schema.is_some() {
            parts.push(format!("{} in {}", count(Some(side)), ws.slot(side).name));
        }
    }
    if analysis.is_some() {
        parts.push(format!("{} in fitting them together", count(None)));
    }
    let findings_text = format!(
        "{} findings, one per row with severity, database, rule, object, explanation and what to do: {}.",
        findings.len(),
        parts.join(", ")
    );
    let report = report_markdown(&ws, analysis.as_ref());
    let html = report_html(&ws, analysis.as_ref());
    let csv = findings_csv(&ws, &findings);
    let table = findings_table(&ws, &findings);
    let mapping = analysis.as_ref().map(|a| mapping_csv(&ws, a));
    let sql_available = ws.sql_available();
    let sql = analysis
        .as_ref()
        .filter(|_| sql_available)
        .map(|a| ws.draft_sql(a));
    let saved = save_workspace(&ws);
    drop(ws);
    drop(analysis);

    let html_name = format!("{stem}-report.html");
    let (report_name, csv_name, mapping_name, sql_name, ws_name) = (
        format!("{stem}-report.md"),
        format!("{stem}-findings.csv"),
        format!("{stem}-mapping.csv"),
        format!("{stem}-draft.sql"),
        format!("{stem}.schema-bridge.json"),
    );
    let report_save = report.clone();

    rsx! {
        {head}
        div { class: "export-grid",
            ExportCard {
                title: "Comparison report",
                text: "The complete deliverable: summary, problems, every mapping with its type conversion, changes made by hand, the integration plan, the draft SQL, warnings, assumptions and unresolved items. The HTML file opens in any browser and prints to PDF; the Markdown file suits wikis and tickets.",
                button {
                    class: "btn primary",
                    onclick: move |_| download_text(&html_name, "text/html", html.clone()),
                    Icon { name: "export", size: 15 }
                    "Save report (HTML)"
                }
                button {
                    class: "btn secondary",
                    onclick: move |_| download_text(&report_name, "text/markdown", report_save.clone()),
                    "Save as Markdown"
                }
                CopyButton { text: report.clone(), label: "Copy Markdown" }
            }
            ExportCard { title: "Findings", text: findings_text,
                button {
                    class: "btn primary",
                    onclick: move |_| download_text(&csv_name, "text/csv", csv.clone()),
                    "Save as CSV"
                }
                CopyButton { text: table, label: "Copy as table" }
            }
            ExportCard {
                title: "Mapping",
                text: "Every source column and where it goes: types, type conversion and its explanation, how it was matched and with what confidence, whether you changed it, and notes. Plus the required target columns nothing fills. For a spreadsheet review.",
                if let Some(mapping) = mapping {
                    button {
                        class: "btn primary",
                        onclick: move |_| download_text(&mapping_name, "text/csv", mapping.clone()),
                        "Save as CSV"
                    }
                } else {
                    span { class: "muted", "Needs both databases." }
                }
            }
            ExportCard {
                title: "Draft SQL",
                text: "The test-load script in load order, every open decision marked TODO. For review only; Schema Bridge never runs it.",
                if let Some(sql) = sql {
                    button {
                        class: "btn primary",
                        onclick: move |_| download_text(&sql_name, "application/sql", sql.clone()),
                        "Save as .sql"
                    }
                    button { class: "btn ghost", onclick: move |_| go_to_sql.call(()), "Review it first →" }
                } else if !sql_available {
                    span { class: "muted", "Not available for a time-series target." }
                } else {
                    span { class: "muted", "Needs both databases." }
                }
            }
        }
        section { class: "card export-card workspace-card",
            div {
                h3 { "Saved comparison" }
                p { class: "muted",
                    "Save the comparison to continue later or hand it to a colleague: both structure exports, the names, the word list and every choice you made. Open it again with “Open workspace…”. The file stays on your computer and holds structure only: no table contents, no passwords, no connection details."
                }
            }
            div { class: "export-actions",
                button {
                    class: "btn primary",
                    onclick: move |_| download_text(&ws_name, "application/json", saved.clone()),
                    Icon { name: "save", size: 15 }
                    "Save comparison"
                }
                OpenWorkspaceButton { workspace, id: "open-workspace-export", class: "btn secondary" }
            }
        }
        details { class: "card report-preview",
            summary { "Preview the report" }
            pre { class: "sql", tabindex: "0", {report} }
        }
    }
}
