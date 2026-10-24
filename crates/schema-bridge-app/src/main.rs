//! Schema Bridge application. Runs in the browser as WebAssembly, so loaded
//! files never leave the user's computer.

mod browser;
mod export;
mod icons;
mod integration;
mod matching;
mod plan;
mod problems;
mod report;
mod state;
mod views;
mod widgets;

use dioxus::prelude::*;

use browser::{download_text, file_stem};
use export::ExportStage;
use icons::Icon;
use integration::IntegrationStage;
use matching::{MatchState, MatchingStage};
use plan::{PlanStage, SqlStage};
use problems::Problems;
use report::ReportStage;
use schema_bridge_core::workspace::{FindingFilter, Workspace};
use schema_bridge_core::workspace_file::save_workspace;
use views::LoadStage;

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    dioxus::launch(App);
}

/// The workflow stages, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Load,
    Problems,
    Matching,
    Integration,
    Plan,
    Sql,
    Report,
    Export,
}

impl Stage {
    const ALL: [Stage; 8] = [
        Stage::Load,
        Stage::Problems,
        Stage::Matching,
        Stage::Integration,
        Stage::Plan,
        Stage::Sql,
        Stage::Report,
        Stage::Export,
    ];

    fn label(self) -> &'static str {
        match self {
            Stage::Load => "Load schemas",
            Stage::Problems => "Problems",
            Stage::Matching => "Matching",
            Stage::Integration => "Integration issues",
            Stage::Plan => "Plan",
            Stage::Sql => "Draft SQL",
            Stage::Report => "Report",
            Stage::Export => "Export",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Stage::Load => "Source and target structure",
            Stage::Problems => "Checks inside each database",
            Stage::Matching => "Tables and columns, with overrides",
            Stage::Integration => "What does not fit, in plain words",
            Stage::Plan => "Steps and load order",
            Stage::Sql => "For review, never run by the app",
            Stage::Report => "The whole comparison at a glance",
            Stage::Export => "Report, findings, saved comparison",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Stage::Load => "load",
            Stage::Problems => "problems",
            Stage::Matching => "matching",
            Stage::Integration => "integration",
            Stage::Plan => "plan",
            Stage::Sql => "sql",
            Stage::Report => "report",
            Stage::Export => "export",
        }
    }
}

/// What the navigation shows for one stage.
struct StepView {
    stage: Stage,
    number: String,
    class: String,
    current: &'static str,
    /// Number shown next to the label (findings for Problems), or empty.
    count: String,
    count_class: String,
    /// Shows a check mark instead of the icon.
    done: bool,
    /// The hint, or what the step is waiting for.
    hint: &'static str,
}

/// The colour theme: the system's, or chosen by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Theme {
    System,
    Light,
    Dark,
}

impl Theme {
    const ALL: [Theme; 3] = [Theme::System, Theme::Light, Theme::Dark];

    fn label(self) -> &'static str {
        match self {
            Theme::System => "System",
            Theme::Light => "Light",
            Theme::Dark => "Dark",
        }
    }

    /// Value of the `data-theme` attribute the stylesheet reads.
    fn attribute(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }
}

/// Colour tone of a count badge: the most serious severity it contains.
fn tone_of(worst: Option<schema_bridge_core::Severity>) -> &'static str {
    use schema_bridge_core::Severity::*;
    match worst {
        Some(Blocking | High) => "error",
        Some(Medium) => "warning",
        Some(Low) => "info",
        _ => "neutral",
    }
}

#[component]
fn App() -> Element {
    let workspace = use_signal(Workspace::new);
    let derived = state::provide(workspace);
    let mut stage = use_signal(|| Stage::Load);
    let filter = use_signal(FindingFilter::default);
    let integration_filter = use_signal(FindingFilter::default);
    let match_state = use_signal(MatchState::default);
    let sql_scope = use_signal(|| None::<String>);
    let mut theme = use_signal(|| Theme::System);
    // Stages the user has opened; with their data available they count as done.
    let mut visited = use_signal(|| vec![Stage::Load]);

    let current_stage = stage();
    let ws = workspace.read();
    let loaded = (ws.source.schema.is_some(), ws.target.schema.is_some());
    let both = loaded.0 && loaded.1;
    let context = (loaded.0 || loaded.1).then(|| {
        (
            ws.source.name.clone(),
            ws.target.name.clone(),
            ws.using_samples,
        )
    });
    let has_context = context.is_some();
    let save_name = format!(
        "{}-into-{}.schema-bridge.json",
        file_stem(&ws.source.name),
        file_stem(&ws.target.name)
    );
    drop(ws);

    let findings = derived.findings.read();
    let analysis = derived.analysis.read();
    let issues = analysis.as_ref().map_or(0, |a| a.integration.len());
    let worst_issue = analysis
        .as_ref()
        .and_then(|a| a.integration.iter().map(|f| f.severity).min());
    let worst_finding = findings.iter().map(|(_, f)| f.severity).min();
    let mapped = analysis
        .as_ref()
        .map(|a| format!("{}/{}", a.matching.mappings.len(), a.matching.tables.len()));
    let finding_count = findings.len();
    drop(findings);
    drop(analysis);

    let seen = visited.read().clone();
    let steps: Vec<StepView> = Stage::ALL
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            // Which data the stage needs: none (Load), one database, or both.
            let available = match s {
                Stage::Load => true,
                Stage::Problems | Stage::Report | Stage::Export => loaded.0 || loaded.1,
                _ => both,
            };
            let complete = match s {
                Stage::Load => both,
                _ => available && seen.contains(&s),
            };
            let state = match s {
                _ if s == current_stage && complete => "current done",
                _ if s == current_stage => "current",
                _ if complete => "done",
                _ if available => "ready",
                _ => "waiting",
            };
            let hint = match (available, s) {
                (true, _) => s.hint(),
                (false, Stage::Problems | Stage::Report | Stage::Export) => "Needs a schema",
                (false, _) => "Needs both schemas",
            };
            let (count, tone) = match s {
                Stage::Problems if finding_count > 0 => {
                    (finding_count.to_string(), tone_of(worst_finding))
                }
                Stage::Integration if issues > 0 => (issues.to_string(), tone_of(worst_issue)),
                Stage::Matching => (mapped.clone().unwrap_or_default(), "neutral"),
                _ => (String::new(), "neutral"),
            };
            StepView {
                stage: s,
                number: (i + 1).to_string(),
                class: format!("step {state}"),
                current: if s == current_stage { "step" } else { "false" },
                count,
                count_class: format!("step-count tone-{tone}"),
                done: complete,
                hint,
            }
        })
        .collect();
    let completed = steps.iter().filter(|s| s.done).count();
    let progress = format!(
        "width: {:.1}%",
        100.0 * completed as f64 / Stage::ALL.len() as f64
    );
    let progress_text = format!("{completed} of {} complete", Stage::ALL.len());
    let position = Stage::ALL
        .iter()
        .position(|s| *s == current_stage)
        .unwrap_or(0)
        + 1;
    let eyebrow = format!("Step {position} of {}", Stage::ALL.len());
    let theme_value = theme().attribute();
    let mut open = move |s: Stage| {
        stage.set(s);
        if !visited.read().contains(&s) {
            visited.write().push(s);
        }
    };
    let go = move |s: Stage| EventHandler::new(move |()| open(s));

    rsx! {
        document::Title { "Schema Bridge" }
        Stylesheet { href: MAIN_CSS }
        div { class: "app", "data-theme": theme_value,
            header { class: "topbar",
                div { class: "brand",
                    span { class: "brand-mark", aria_hidden: "true",
                        Icon { name: "bridge", size: 20 }
                    }
                    div {
                        h1 { "Schema Bridge" }
                        p { "Compare database schemas · plan the integration" }
                    }
                }
                if let Some((source, target, samples)) = context {
                    div { class: "context", aria_label: "Current comparison",
                        span { class: "context-db src",
                            span { class: "id-dot", aria_hidden: "true" }
                            {source}
                        }
                        span { class: "context-arrow", aria_hidden: "true", "→" }
                        span { class: "context-db tgt",
                            span { class: "id-dot", aria_hidden: "true" }
                            {target}
                        }
                        if samples {
                            span { class: "badge info", "Sample data" }
                        }
                    }
                }
                div { class: "topbar-end",
                    p { class: "local-note",
                        span { class: "dot", aria_hidden: "true" }
                        "Runs locally"
                    }
                    if has_context {
                        button {
                            class: "btn secondary small",
                            title: "Save the comparison (both exports, word list and your choices) as a file on this computer",
                            onclick: move |_| {
                                let text = save_workspace(&workspace.read());
                                download_text(&save_name, "application/json", text);
                            },
                            Icon { name: "save", size: 15 }
                            "Save comparison"
                        }
                    }
                    div { class: "seg theme-switch", role: "group", aria_label: "Colour theme",
                        for t in Theme::ALL {
                            button {
                                key: "{t.label()}",
                                aria_pressed: if theme() == t { "true" } else { "false" },
                                title: "{t.label()} theme",
                                onclick: move |_| theme.set(t),
                                {t.label()}
                            }
                        }
                    }
                }
            }
            div { class: "body",
                nav { class: "sidebar", aria_label: "Workflow",
                    div { class: "progress",
                        div { class: "progress-head",
                            span { class: "nav-heading", "Workflow" }
                            span { class: "progress-text", {progress_text} }
                        }
                        div { class: "progress-track", aria_hidden: "true",
                            span { class: "progress-fill", style: progress }
                        }
                    }
                    ol { class: "steps",
                        for StepView { stage: s , number , class , current , count , count_class , done , hint } in steps {
                            li { key: "{number}", class: if done { "is-done" } else { "" },
                                button {
                                    class,
                                    aria_current: current,
                                    onclick: move |_| open(s),
                                    span { class: "step-num", aria_hidden: "true",
                                        if done { Icon { name: "check", size: 15 } } else { Icon { name: s.icon(), size: 15 } }
                                    }
                                    span { class: "step-text",
                                        span { class: "step-label",
                                            span { class: "visually-hidden", "Step {number}: " }
                                            span { class: "step-no", "{number}" }
                                            {s.label()}
                                            if !count.is_empty() {
                                                span { class: count_class, {count} }
                                            }
                                        }
                                        span { class: "step-hint", {hint} }
                                    }
                                }
                            }
                        }
                    }
                    p { class: "sidebar-note",
                        "Schema Bridge reads structure exports only. It never connects to a database and never runs the SQL it drafts."
                    }
                }
                main { class: "content", key: "{position}",
                    p { class: "eyebrow",
                        Icon { name: current_stage.icon(), size: 14 }
                        {eyebrow}
                    }
                    {
                        match current_stage {
                            Stage::Load => rsx! {
                                LoadStage {
                                    workspace,
                                    open_problems: go(Stage::Problems),
                                    open_matching: go(Stage::Matching),
                                    open_report: go(Stage::Report),
                                }
                            },
                            Stage::Problems => rsx! {
                                Problems { workspace, filter, go_to_load: go(Stage::Load) }
                            },
                            Stage::Matching => rsx! {
                                MatchingStage { workspace, state: match_state, go_to_load: go(Stage::Load) }
                            },
                            Stage::Integration => rsx! {
                                IntegrationStage {
                                    workspace,
                                    filter: integration_filter,
                                    go_to_load: go(Stage::Load),
                                    go_to_matching: go(Stage::Matching),
                                    go_to_plan: go(Stage::Plan),
                                }
                            },
                            Stage::Plan => rsx! {
                                PlanStage { workspace, go_to_load: go(Stage::Load), go_to_sql: go(Stage::Sql) }
                            },
                            Stage::Sql => rsx! {
                                SqlStage {
                                    workspace,
                                    scope: sql_scope,
                                    go_to_load: go(Stage::Load),
                                    go_to_plan: go(Stage::Plan),
                                }
                            },
                            Stage::Report => rsx! {
                                ReportStage {
                                    workspace,
                                    go_to_load: go(Stage::Load),
                                    go_to_matching: go(Stage::Matching),
                                    go_to_export: go(Stage::Export),
                                }
                            },
                            Stage::Export => rsx! {
                                ExportStage { workspace, go_to_load: go(Stage::Load), go_to_sql: go(Stage::Sql) }
                            },
                        }
                    }
                }
            }
        }
    }
}
