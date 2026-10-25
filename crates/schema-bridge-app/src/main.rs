//! Schema Bridge application. Runs in the browser as WebAssembly, so loaded
//! files never leave the user's computer.

mod browser;
mod export;
mod feedback;
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

use browser::{download_text, file_stem, reveal_stage, save_theme, saved_theme};
use export::ExportStage;
use feedback::{ConfirmDialog, Notices};
use icons::Icon;
use integration::IntegrationStage;
use matching::{MatchState, MatchingStage};
use plan::{PlanStage, SqlStage};
use problems::Problems;
use report::ReportStage;
use schema_bridge_core::report::{Summary, summary};
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
    /// What the step still needs, its usual hint, or what it waits for.
    hint: String,
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

    fn from_attribute(value: &str) -> Option<Theme> {
        Theme::ALL.into_iter().find(|t| t.attribute() == value)
    }
}

/// Whether a stage's own check passes: `Some(true)` when nothing is open
/// there, `Some(false)` when something is, `None` for stages without a
/// check (Report, Export, or Draft SQL for a time-series target). The
/// counts are the report's, so the navigation and the report agree.
fn stage_check(stage: Stage, s: &Summary, both: bool, sql_available: bool) -> Option<bool> {
    let count = |kind: &str| s.unresolved.iter().filter(|u| u.kind == kind).count();
    match stage {
        Stage::Load => Some(both),
        _ if !both => None,
        Stage::Problems => Some(count("Fix first") == 0),
        Stage::Matching => Some(count("Review") == 0 && s.tables.no_place == 0),
        Stage::Integration => Some(s.risks.is_empty()),
        Stage::Plan => Some(count("Decide") == 0),
        Stage::Sql if sql_available => Some(s.todos == 0),
        _ => None,
    }
}

/// What a stage still needs, shown under its name instead of the usual hint.
fn open_items(stage: Stage, s: &Summary, sql_available: bool) -> Option<String> {
    let count = |kind: &str| s.unresolved.iter().filter(|u| u.kind == kind).count();
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    match stage {
        Stage::Problems if count("Fix first") > 0 => Some(format!(
            "{} to fix first",
            plural(count("Fix first"), "blocking problem", "blocking problems")
        )),
        Stage::Matching => {
            let mut parts = Vec::new();
            if count("Review") > 0 {
                parts.push(plural(
                    count("Review"),
                    "match to review",
                    "matches to review",
                ));
            }
            if s.tables.no_place > 0 {
                parts.push(format!("{} without a place", s.tables.no_place));
            }
            (!parts.is_empty()).then(|| parts.join(" · "))
        }
        Stage::Integration if !s.risks.is_empty() => Some(format!(
            "{} blocking or high",
            plural(s.risks.len(), "issue", "issues")
        )),
        Stage::Plan if count("Decide") > 0 => {
            Some(plural(count("Decide"), "decision open", "decisions open"))
        }
        Stage::Sql if !sql_available => Some("Not available for a time-series target".into()),
        Stage::Sql if s.todos > 0 => {
            Some(format!("{} to resolve", plural(s.todos, "TODO", "TODOs")))
        }
        _ => None,
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
    // The report's counts, which the navigation's readiness is built from.
    let report = use_memo(move || summary(&workspace.read(), derived.analysis.read().as_ref()));
    // The theme chosen in an earlier session.
    use_future(move || async move {
        if let Some(t) = saved_theme()
            .await
            .as_deref()
            .and_then(Theme::from_attribute)
        {
            theme.set(t);
        }
    });

    let current_stage = stage();
    let ws = workspace.read();
    let loaded = (ws.source.schema.is_some(), ws.target.schema.is_some());
    let both = loaded.0 && loaded.1;
    let sql_available = ws.sql_available();
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

    let s = report.read();
    let available = |st: Stage| match st {
        Stage::Load => true,
        Stage::Problems | Stage::Report | Stage::Export => loaded.0 || loaded.1,
        _ => both,
    };
    let steps: Vec<StepView> = Stage::ALL
        .into_iter()
        .enumerate()
        .map(|(i, st)| {
            let ready = available(st);
            let complete = stage_check(st, &s, both, sql_available) == Some(true);
            let state = match st {
                _ if st == current_stage && complete => "current done",
                _ if st == current_stage => "current",
                _ if complete => "done",
                _ if ready => "ready",
                _ => "waiting",
            };
            let hint = match (ready, st) {
                (true, _) => open_items(st, &s, sql_available).unwrap_or_else(|| st.hint().into()),
                (false, Stage::Problems | Stage::Report | Stage::Export) => "Needs a schema".into(),
                (false, _) => "Needs both schemas".into(),
            };
            let (count, tone) = match st {
                Stage::Problems if finding_count > 0 => {
                    (finding_count.to_string(), tone_of(worst_finding))
                }
                Stage::Integration if issues > 0 => (issues.to_string(), tone_of(worst_issue)),
                Stage::Matching => (mapped.clone().unwrap_or_default(), "neutral"),
                Stage::Sql if both && sql_available && s.todos > 0 => {
                    (s.todos.to_string(), "warning")
                }
                _ => (String::new(), "neutral"),
            };
            StepView {
                stage: st,
                number: (i + 1).to_string(),
                class: format!("step {state}"),
                current: if st == current_stage { "step" } else { "false" },
                count,
                count_class: format!("step-count tone-{tone}"),
                done: complete,
                hint,
            }
        })
        .collect();

    // Readiness: the stage checks that pass, and the report's verdict.
    let checks: Vec<bool> = Stage::ALL
        .into_iter()
        .filter_map(|st| stage_check(st, &s, both, sql_available))
        .collect();
    let clear = checks.iter().filter(|c| **c).count();
    let open = s
        .unresolved
        .iter()
        .filter(|u| u.kind == "Fix first" || u.kind == "Decide")
        .count();
    drop(s);
    let progress = format!("width: {:.1}%", 100.0 * clear as f64 / checks.len() as f64);
    // The count only means something once both schemas are compared.
    let progress_text = if both {
        format!("{clear} of {} checks clear", checks.len())
    } else {
        String::new()
    };
    let (readiness_class, readiness) = match (loaded, open) {
        ((false, false), _) => ("readiness", "Load two schemas to begin".to_string()),
        ((true, false) | (false, true), _) => {
            ("readiness", "Load the other schema to compare".to_string())
        }
        (_, 0) => ("readiness good", "Ready for a test load".to_string()),
        (_, 1) => (
            "readiness open",
            "1 item to resolve before a test load".to_string(),
        ),
        (_, n) => (
            "readiness open",
            format!("{n} items to resolve before a test load"),
        ),
    };
    let fill_class = if clear == checks.len() {
        "progress-fill all-clear"
    } else {
        "progress-fill"
    };

    let index = Stage::ALL
        .iter()
        .position(|st| *st == current_stage)
        .unwrap_or(0);
    let position = index + 1;
    let eyebrow = format!("Step {position} of {}", Stage::ALL.len());
    let previous = index.checked_sub(1).map(|i| Stage::ALL[i]);
    let next = Stage::ALL.get(index + 1).copied();
    let next_available = next.is_some_and(available);
    let next_hint = match next {
        Some(Stage::Problems | Stage::Report | Stage::Export) => "Needs a schema",
        _ => "Needs both schemas",
    };
    let theme_value = theme().attribute();
    let mut open = move |st: Stage| {
        if stage() != st {
            stage.set(st);
            reveal_stage();
        }
    };
    let go = move |st: Stage| EventHandler::new(move |()| open(st));

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
                                onclick: move |_| {
                                    theme.set(t);
                                    save_theme(t.attribute());
                                },
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
                            span { class: "nav-heading", "Readiness" }
                            span { class: "progress-text", {progress_text} }
                        }
                        div { class: "progress-track", aria_hidden: "true",
                            span { class: fill_class, style: progress }
                        }
                        p { class: readiness_class, role: "status", {readiness} }
                    }
                    ol { class: "steps",
                        for StepView { stage: st , number , class , current , count , count_class , done , hint } in steps {
                            li { key: "{number}", class: if done { "is-done" } else { "" },
                                button {
                                    class,
                                    aria_current: current,
                                    onclick: move |_| open(st),
                                    span { class: "step-num", aria_hidden: "true",
                                        if done { Icon { name: "check", size: 15 } } else { Icon { name: st.icon(), size: 15 } }
                                    }
                                    span { class: "step-text",
                                        span { class: "step-label",
                                            span { class: "visually-hidden", "Step {number}: " }
                                            span { class: "step-no", "{number}" }
                                            {st.label()}
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
                    nav { class: "stage-nav", aria_label: "Previous and next stage",
                        if let Some(p) = previous {
                            button {
                                class: "btn ghost stage-prev",
                                onclick: move |_| open(p),
                                span { class: "stage-nav-dir", "← Previous" }
                                span { class: "stage-nav-name", {p.label()} }
                            }
                        } else {
                            span {}
                        }
                        if let Some(n) = next {
                            button {
                                class: "btn secondary stage-next",
                                disabled: !next_available,
                                title: if next_available { "" } else { next_hint },
                                onclick: move |_| open(n),
                                span { class: "stage-nav-dir",
                                    if next_available { "Next →" } else { "Next · {next_hint}" }
                                }
                                span { class: "stage-nav-name", {n.label()} }
                            }
                        }
                    }
                }
            }
            ConfirmDialog { workspace }
            Notices {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schema_bridge_core::TableChoice;

    fn checks(ws: &Workspace) -> Vec<(Stage, Option<bool>)> {
        let a = ws.analysis();
        let s = summary(ws, a.as_ref());
        let both = ws.source.schema.is_some() && ws.target.schema.is_some();
        Stage::ALL
            .into_iter()
            .map(|st| (st, stage_check(st, &s, both, ws.sql_available())))
            .collect()
    }

    #[test]
    fn visiting_a_stage_does_not_complete_it() {
        let mut ws = Workspace::new();
        assert_eq!(checks(&ws)[0], (Stage::Load, Some(false)));
        assert!(checks(&ws)[1..].iter().all(|(_, c)| c.is_none()));

        // The sample has a blocking problem, uncertain matches, open
        // decisions and TODOs: only loading is done.
        ws.load_samples();
        let c = checks(&ws);
        assert_eq!(
            c,
            [
                (Stage::Load, Some(true)),
                (Stage::Problems, Some(false)),
                (Stage::Matching, Some(false)),
                (Stage::Integration, Some(false)),
                (Stage::Plan, Some(false)),
                (Stage::Sql, Some(false)),
                (Stage::Report, None),
                (Stage::Export, None),
            ]
        );
        let a = ws.analysis();
        let s = summary(&ws, a.as_ref());
        assert_eq!(
            open_items(Stage::Problems, &s, true).as_deref(),
            Some("1 blocking problem to fix first")
        );
        assert_eq!(
            open_items(Stage::Sql, &s, true),
            Some(format!("{} TODOs to resolve", s.todos))
        );
    }

    #[test]
    fn reviewing_every_uncertain_match_clears_the_matching_check() {
        let mut ws = Workspace::new();
        ws.load_samples();
        // Accept every uncertain table match, then every uncertain column.
        let a = ws.analysis().unwrap();
        for t in a.matching.tables.iter().filter(|t| t.is_uncertain()) {
            let target = t.target.clone().unwrap();
            ws.set_table_choice(&t.source, Some(TableChoice::Target(target)));
        }
        let a = ws.analysis().unwrap();
        for p in &a.matching.mappings {
            for r in p.rows.iter().filter(|r| r.is_uncertain()) {
                let target = r.target.as_ref().map(|c| c.name.clone());
                ws.set_column_choice(&p.source, &r.source.name, Some(target));
            }
        }
        let c = checks(&ws);
        assert_eq!(c[2], (Stage::Matching, Some(true)));
        // The other checks are unaffected by reviewing.
        assert_eq!(c[1], (Stage::Problems, Some(false)));
    }

    #[test]
    fn a_time_series_target_has_no_sql_check() {
        let mut ws = Workspace::new();
        ws.load_timeseries_samples();
        ws.swap();
        let c = checks(&ws);
        assert_eq!(c[5], (Stage::Sql, None));
        let a = ws.analysis();
        let s = summary(&ws, a.as_ref());
        assert_eq!(
            open_items(Stage::Sql, &s, false).as_deref(),
            Some("Not available for a time-series target")
        );
    }
}
