//! The "Matching" stage: which source table goes into which target table,
//! column by column. Users search and filter the tables, review uncertain
//! matches, and accept, change or remove any match by hand; the word list is
//! a second view. Matching itself, the filters and the reasons come from the
//! core library; this file prepares strings and places them.

use dioxus::prelude::*;
use schema_bridge_core::matching::{
    ColumnFilter, ColumnHow, ColumnReview, ColumnRow, ColumnStatus, DecidedBy, MatchKind, Review,
    TableChoice, TableFilter, TableSort, WordList,
};
use schema_bridge_core::workspace::{Workspace, grouped};
use schema_bridge_core::{Fit, Severity, table_name};

use crate::browser::{CopyButton, download_text};
use crate::icons::Icon;
use crate::problems::more_label;
use crate::state::derived;

/// Tables listed at first; more on request, so huge schemas stay fast.
const LIST_PAGE: usize = 200;

/// Which part of the Matching stage is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MatchView {
    #[default]
    Tables,
    Words,
}

/// Everything the Matching stage remembers while the user moves around.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MatchState {
    pub view: MatchView,
    pub selected: Option<String>,
    pub tables: TableFilter,
    pub sort: TableSort,
    pub columns: ColumnFilter,
    /// How many tables the list shows (grows with "Show more").
    pub shown: usize,
}

/// CSS class of a column or table status: `st-ok`, `st-convert`, …
pub fn status_class(status: ColumnStatus) -> String {
    format!("st-{}", status.code())
}

fn note_class(severity: Severity) -> &'static str {
    match severity {
        Severity::Blocking | Severity::High => "note high",
        Severity::Medium => "note medium",
        _ => "note low",
    }
}

fn percent(score: f64) -> String {
    format!("{:.0}%", score.clamp(0.0, 1.0) * 100.0)
}

fn pressed(on: bool) -> &'static str {
    if on { "true" } else { "false" }
}

#[component]
pub fn MatchingStage(
    workspace: Signal<Workspace>,
    state: Signal<MatchState>,
    go_to_load: EventHandler<()>,
) -> Element {
    let mut workspace = workspace;
    let mut state = state;
    let ws = workspace.read();
    let rules = ws.words().rule_count();
    let has_choices = !ws.overrides.is_empty();
    let both = ws.source.schema.is_some() && ws.target.schema.is_some();
    drop(ws);
    let view = state.read().view;
    let words_label = format!("Word list · {rules} rules");

    rsx! {
        section { class: "page-head",
            div {
                h2 { "Matching" }
                p { class: "lead",
                    "Which source tables go into which target tables, column by column. Matches come from names, the word list, descriptions and types. Review the uncertain ones, and accept, change or remove any match: your choices always win."
                }
            }
            div { class: "page-actions",
                button {
                    class: "btn ghost",
                    disabled: !has_choices,
                    title: "Forget every table and column choice made by hand",
                    onclick: move |_| workspace.write().reset_choices(),
                    Icon { name: "undo", size: 15 }
                    "Reset all my choices"
                }
            }
        }
        div { class: "seg view-switch", role: "group", aria_label: "View",
            button {
                aria_pressed: pressed(view == MatchView::Tables),
                onclick: move |_| state.write().view = MatchView::Tables,
                "Tables and columns"
            }
            button {
                aria_pressed: pressed(view == MatchView::Words),
                onclick: move |_| state.write().view = MatchView::Words,
                {words_label}
            }
        }
        if view == MatchView::Words {
            WordListEditor { workspace }
        } else if !both {
            section { class: "card empty",
                p { class: "empty-title", "Matching needs both databases" }
                p { "Load the source and the target export first, or try the sample schemas." }
                div {
                    button { class: "btn primary", onclick: move |_| go_to_load.call(()), "Go to Load schemas" }
                }
            }
        } else {
            ColumnCoverage { workspace }
            TablesAndColumns { workspace, state }
            TypeLegend {}
        }
    }
}

/// One entry of the table list.
struct ListEntry {
    key: String,
    name: String,
    status_class: String,
    status: &'static str,
    target: String,
    current: &'static str,
    flags: Vec<(&'static str, &'static str)>,
}

#[component]
fn TablesAndColumns(workspace: Signal<Workspace>, state: Signal<MatchState>) -> Element {
    let mut state = state;
    let d = derived();
    let guard = d.analysis.read();
    let ws = workspace.read();
    let (Some(source), Some(analysis)) = (ws.source.schema.as_ref(), guard.as_ref()) else {
        return rsx! {};
    };
    let matching = &analysis.matching;
    let st = state.read().clone();

    let list = matching.filtered(source, &st.tables, st.sort);
    let total_listed = list.len();
    let limit = st.shown.max(LIST_PAGE);
    // The selected table, or the first listed one.
    let chosen = st
        .selected
        .clone()
        .filter(|k| source.table(k).is_some())
        .or_else(|| {
            list.iter()
                .find(|m| m.target.is_some())
                .map(|m| m.source.clone())
        })
        .or_else(|| list.first().map(|m| m.source.clone()))
        .or_else(|| source.tables.first().map(|t| t.key.clone()));

    let review_chips: Vec<(String, &'static str, String, &'static str, Option<Review>)> =
        std::iter::once((
            "all".to_string(),
            "All",
            matching.tables.len().to_string(),
            pressed(st.tables.review.is_none()),
            None,
        ))
        .chain(Review::ALL.iter().map(|r| {
            (
                format!("{r:?}"),
                r.label(),
                matching.count(*r).to_string(),
                pressed(st.tables.review == Some(*r)),
                Some(*r),
            )
        }))
        .collect();

    let entries: Vec<ListEntry> = list
        .iter()
        .take(limit)
        .map(|m| {
            let status = match (matching.mapping(&m.source), m.kind) {
                (Some(p), _) => p.status(),
                (None, MatchKind::Stays) => ColumnStatus::NeverCopy,
                (None, _) => ColumnStatus::NotCopied,
            };
            let label = match status {
                ColumnStatus::Copy => "ready",
                ColumnStatus::Convert => "convert",
                ColumnStatus::Check => "check",
                ColumnStatus::NotCopied => "no place",
                ColumnStatus::NeverCopy => "stays",
            };
            let mut flags = Vec::new();
            if matching.is_in(m, Review::Uncertain) {
                flags.push(("flag uncertain", "uncertain"));
            }
            if matching.is_in(m, Review::Manual) {
                flags.push(("flag manual", "changed"));
            }
            ListEntry {
                key: m.source.clone(),
                name: table_name(&m.source).to_string(),
                status_class: format!("pill {}", status_class(status)),
                status: label,
                target: m
                    .target
                    .as_deref()
                    .map(|t| format!("→ {}", table_name(t)))
                    .unwrap_or_default(),
                current: pressed(Some(&m.source) == chosen.as_ref()),
                flags,
            }
        })
        .collect();
    let more = total_listed.saturating_sub(limit);
    let shown_text = if total_listed == matching.tables.len() {
        format!("{} tables", total_listed)
    } else {
        format!("{} of {} tables", total_listed, matching.tables.len())
    };
    let search = st.tables.text.clone();
    let sort = st.sort;
    let detail = chosen.and_then(|k| table_detail(&ws, analysis, &k, &st.columns));
    drop(ws);
    drop(guard);

    rsx! {
        div { class: "match-layout",
            nav { class: "tlist card", aria_label: "Source tables",
                div { class: "tlist-tools",
                    label { class: "search",
                        Icon { name: "search", size: 15 }
                        span { class: "visually-hidden", "Search tables and columns" }
                        input {
                            r#type: "search",
                            placeholder: "Search tables and columns…",
                            value: search,
                            oninput: move |e| {
                                let mut s = state.write();
                                s.tables.text = e.value();
                                s.shown = LIST_PAGE;
                            },
                        }
                    }
                    div { class: "chip-row", role: "group", aria_label: "Show tables",
                        for (key , label , count , is_pressed , value) in review_chips {
                            button {
                                key: "{key}",
                                class: "filter-chip",
                                aria_pressed: is_pressed,
                                onclick: move |_| {
                                    let mut s = state.write();
                                    s.tables.review = value;
                                    s.shown = LIST_PAGE;
                                },
                                {label}
                                span { class: "chip-count", {count} }
                            }
                        }
                    }
                    div { class: "tlist-meta",
                        span { class: "muted", role: "status", {shown_text} }
                        label { class: "sort",
                            span { "Sort" }
                            select {
                                onchange: move |e| {
                                    state.write().sort = match e.value().as_str() {
                                        "attention" => TableSort::Attention,
                                        "size" => TableSort::Size,
                                        _ => TableSort::Name,
                                    };
                                },
                                option { value: "name", selected: sort == TableSort::Name, "Name" }
                                option { value: "attention", selected: sort == TableSort::Attention, "Needs attention" }
                                option { value: "size", selected: sort == TableSort::Size, "Most columns" }
                            }
                        }
                    }
                }
                if entries.is_empty() {
                    p { class: "tlist-empty", "No table matches. Clear the search or choose another filter." }
                }
                div { class: "tlist-items",
                    for ListEntry { key , name , status_class , status , target , current , flags } in entries {
                        button {
                            key: "{key}",
                            class: "tbtn",
                            aria_current: current,
                            onclick: move |_| state.write().selected = Some(key.clone()),
                            span { class: "tbtn-name mono", {name} }
                            span { class: status_class, {status} }
                            span { class: "tbtn-to mono", {target} }
                            span { class: "tbtn-flags",
                                for (class , text) in flags {
                                    span { key: "{text}", class, {text} }
                                }
                            }
                        }
                    }
                }
                if more > 0 {
                    button {
                        class: "btn ghost more",
                        onclick: move |_| state.write().shown = limit + LIST_PAGE,
                        {more_label(more, LIST_PAGE)}
                    }
                }
            }
            if let Some(detail) = detail {
                TableDetailView { workspace, state, detail }
            } else {
                section { class: "card empty",
                    p { "Choose a table on the left." }
                }
            }
        }
    }
}

/// Everything the detail panel shows for one source table.
#[derive(Debug, Clone, PartialEq)]
pub struct TableDetail {
    source_key: String,
    source_table: String,
    target_table: String,
    has_target: bool,
    reason: String,
    confidence: Option<String>,
    uncertain: bool,
    by_user: bool,
    /// The target the tool chose, which "Accept" confirms.
    accept_target: Option<String>,
    facts: String,
    /// (value, label, selected) of the target-table picker.
    table_options: Vec<(String, String, bool)>,
    /// Required target columns nothing fills: (name, type, link note).
    required: Vec<(String, String, String)>,
    required_blocks: bool,
    required_head: String,
    rows: Vec<RowView>,
    rows_total: usize,
    column_chips: Vec<(
        String,
        &'static str,
        String,
        &'static str,
        Option<ColumnReview>,
    )>,
    column_search: String,
    /// Shown when the table has no target.
    message: String,
    plain_columns: Vec<(String, String, String)>,
    source_name: String,
    target_name: String,
    /// Draft SQL for this table, empty without a target.
    sql: String,
}

#[derive(Debug, Clone, PartialEq)]
struct RowView {
    column: String,
    key_note: bool,
    description: String,
    source_type: String,
    /// (value, label, selected) of the target-column picker.
    options: Vec<(String, String, bool)>,
    current_target: Option<String>,
    how_class: &'static str,
    how: String,
    target_description: String,
    target_type: String,
    fit_class: String,
    fit_label: String,
    fit_note: String,
    status_class: String,
    status: &'static str,
    notes: Vec<(&'static str, String)>,
    reason: String,
    uncertain: bool,
    manual: bool,
}

fn type_text(label: &str, data_type: &str, nullable: bool) -> String {
    let t = if label.is_empty() { data_type } else { label };
    if nullable {
        t.to_string()
    } else {
        format!("{t} · required")
    }
}

fn row_view(r: &ColumnRow, primary_key: &[String], target_columns: &[String]) -> RowView {
    let current = r.target.as_ref().map(|c| c.name.clone());
    let mut options = vec![
        (
            "@auto".to_string(),
            "↺ Let the tool decide".to_string(),
            false,
        ),
        (
            "@none".to_string(),
            "— not copied —".to_string(),
            current.is_none(),
        ),
    ];
    options.extend(target_columns.iter().map(|c| {
        (
            format!("col:{c}"),
            c.clone(),
            current.as_deref() == Some(c.as_str()),
        )
    }));
    let (how_class, how) = match (r.manual, r.how, r.evidence) {
        (true, _, _) => ("how manual", "Changed by you".to_string()),
        (_, Some(ColumnHow::WordList), _) => ("how list", "Word list".to_string()),
        (_, Some(ColumnHow::Automatic), Some(e)) => (
            if r.is_uncertain() {
                "how uncertain"
            } else {
                "how auto"
            },
            format!("Automatic · {}", percent(e.confidence())),
        ),
        _ => ("", String::new()),
    };
    let (fit_class, fit_label, fit_note) = match &r.compatibility {
        Some(c) => (
            format!("fit fit-{}", c.fit.code()),
            c.fit.label().to_string(),
            c.note.clone(),
        ),
        None => (String::new(), String::new(), String::new()),
    };
    RowView {
        column: r.source.name.clone(),
        key_note: primary_key.contains(&r.source.name),
        description: r.source.comment.clone().unwrap_or_default(),
        source_type: type_text(&r.source.type_label, &r.source.data_type, r.source.nullable),
        options,
        current_target: current,
        how_class,
        how,
        target_description: r
            .target
            .as_ref()
            .and_then(|c| c.comment.clone())
            .unwrap_or_default(),
        target_type: r
            .target
            .as_ref()
            .map(|c| type_text(&c.type_label, &c.data_type, c.nullable))
            .unwrap_or_default(),
        fit_class,
        fit_label,
        fit_note,
        status_class: format!("pill {}", status_class(r.status)),
        status: r.status.label(),
        notes: r
            .notes
            .iter()
            .filter(|n| n.kind != schema_bridge_core::matching::NoteKind::Type)
            .map(|n| (note_class(n.severity), n.text.clone()))
            .collect(),
        reason: r.reason(),
        uncertain: r.is_uncertain(),
        manual: r.manual,
    }
}

fn table_detail(
    ws: &Workspace,
    analysis: &schema_bridge_core::workspace::Analysis,
    key: &str,
    column_filter: &ColumnFilter,
) -> Option<TableDetail> {
    let (source, target) = (ws.source.schema.as_ref()?, ws.target.schema.as_ref()?);
    let matching = &analysis.matching;
    let s = source.table(key)?;
    let m = matching.table(key)?;
    let p = matching.mapping(key);
    let by_user = m.decided_by == DecidedBy::User;
    let (source_name, target_name) = (&ws.source.name, &ws.target.name);

    let mut table_options = vec![
        (
            "@auto".to_string(),
            "Let the tool decide".to_string(),
            !by_user,
        ),
        (
            "@none".to_string(),
            "No place in the target".to_string(),
            by_user && m.kind == MatchKind::NoTarget,
        ),
        (
            "@stays".to_string(),
            "Stays in the source".to_string(),
            by_user && m.kind == MatchKind::Stays,
        ),
    ];
    table_options.extend(target.tables.iter().map(|t| {
        (
            format!("t:{}", t.key),
            format!("→ {}", t.name),
            by_user && m.target.as_deref() == Some(t.key.as_str()),
        )
    }));

    let mut facts = format!("{} columns", s.columns.len());
    if let Some(rows) = s.rows.filter(|r| *r > 0) {
        facts.push_str(&format!(" · about {} rows", grouped(rows)));
    }
    if let Some(c) = s.comment.as_deref().filter(|c| !c.is_empty()) {
        facts.push_str(&format!(" · {c}"));
    }

    let mut detail = TableDetail {
        source_key: s.key.clone(),
        source_table: s.name.clone(),
        target_table: m
            .target
            .as_deref()
            .map_or("—".to_string(), |t| table_name(t).to_string()),
        has_target: p.is_some(),
        reason: m.reason(),
        confidence: m.score.map(percent),
        uncertain: m.is_uncertain(),
        by_user,
        accept_target: m.target.clone().filter(|_| m.kind == MatchKind::Automatic),
        facts,
        table_options,
        required: Vec::new(),
        required_blocks: false,
        required_head: String::new(),
        rows: Vec::new(),
        rows_total: 0,
        column_chips: Vec::new(),
        column_search: column_filter.text.clone(),
        message: String::new(),
        plain_columns: Vec::new(),
        source_name: source_name.clone(),
        target_name: target_name.clone(),
        sql: String::new(),
    };

    let Some(p) = p else {
        detail.message = if m.kind == MatchKind::Stays {
            format!(
                "This table stays in {source_name}. Choose a target table above to bring it in after all."
            )
        } else {
            let closest = m
                .candidate
                .as_ref()
                .map(|c| format!(" The closest candidate is {}.", table_name(&c.target)))
                .unwrap_or_default();
            format!(
                "There is no table in {target_name} for this data yet. Choose one above, or plan a new table.{closest}"
            )
        };
        detail.plain_columns = s
            .columns
            .iter()
            .map(|c| {
                (
                    c.name.clone(),
                    type_text(&c.type_label, &c.data_type, true),
                    c.comment.clone().unwrap_or_default(),
                )
            })
            .collect();
        return Some(detail);
    };
    let t = target.table(&p.target)?;
    detail.sql = ws.table_sql(p);
    detail.required = p
        .required
        .iter()
        .map(|r| {
            (
                r.column.name.clone(),
                type_text(&r.column.type_label, &r.column.data_type, true),
                r.link_to
                    .as_deref()
                    .map(|k| {
                        format!(
                            " — a link to {}. Someone must decide this for every row.",
                            table_name(k)
                        )
                    })
                    .unwrap_or_default(),
            )
        })
        .collect();
    detail.required_blocks = p.required.iter().any(|r| r.link_to.is_some());
    detail.required_head = format!("{} needs values that {source_name} does not have:", t.name);

    let target_columns: Vec<String> = t.columns.iter().map(|c| c.name.clone()).collect();
    detail.rows_total = p.rows.len();
    detail.rows = p
        .rows
        .iter()
        .filter(|r| column_filter.matches(r))
        .map(|r| row_view(r, &s.primary_key, &target_columns))
        .collect();
    detail.column_chips = std::iter::once((
        "all".to_string(),
        "All",
        p.rows.len().to_string(),
        pressed(column_filter.review.is_none()),
        None,
    ))
    .chain(ColumnReview::ALL.iter().map(|r| {
        (
            format!("{r:?}"),
            r.label(),
            p.rows.iter().filter(|row| r.holds(row)).count().to_string(),
            pressed(column_filter.review == Some(*r)),
            Some(*r),
        )
    }))
    .collect();
    Some(detail)
}

#[component]
fn TableDetailView(
    workspace: Signal<Workspace>,
    state: Signal<MatchState>,
    detail: TableDetail,
) -> Element {
    let mut workspace = workspace;
    let mut state = state;
    let TableDetail {
        source_key,
        source_table,
        target_table,
        has_target,
        reason,
        confidence,
        uncertain,
        by_user,
        accept_target,
        facts,
        table_options,
        required,
        required_blocks,
        required_head,
        rows,
        rows_total,
        column_chips,
        column_search,
        message,
        plain_columns,
        source_name,
        target_name,
        sql,
    } = detail;
    let picker_key = source_key.clone();
    let accept_key = source_key.clone();
    let reset_key = source_key.clone();
    let callout_class = if required_blocks {
        "callout error"
    } else {
        "callout warning"
    };
    let source_head = format!("{source_name} column");
    let target_head = format!("{target_name} column");
    let shown = rows.len();

    rsx! {
        section { class: "detail",
            div { class: "card pair-card",
                div { class: "pair-head",
                    span { class: "pair-name src mono", {source_table.clone()} }
                    span { class: "pair-arrow", aria_hidden: "true", Icon { name: "arrow", size: 18 } }
                    span { class: "pair-name tgt mono", {target_table} }
                    if uncertain {
                        span { class: "flag uncertain", "uncertain" }
                    }
                    if by_user {
                        span { class: "flag manual", "changed by you" }
                    }
                }
                div { class: "pair-reason",
                    if let Some(c) = confidence {
                        span { class: "confidence", title: "How similar the tables are by name and columns",
                            span { class: "confidence-bar", style: "--value: {c}" }
                            "{c}"
                        }
                    }
                    p { "{reason} {facts}" }
                }
                div { class: "pair-actions",
                    label { class: "pair-picker",
                        span { class: "field-label small", "Target table" }
                        select {
                            onchange: move |e| {
                                let v = e.value();
                                let choice = match v.as_str() {
                                    "@auto" => None,
                                    "@none" => Some(TableChoice::NoTarget),
                                    "@stays" => Some(TableChoice::Stays),
                                    other => other.strip_prefix("t:").map(|k| TableChoice::Target(k.to_string())),
                                };
                                workspace.write().set_table_choice(&picker_key, choice);
                            },
                            for (value , label , sel) in table_options {
                                option { key: "{value}", value: "{value}", selected: sel, {label} }
                            }
                        }
                    }
                    if let Some(t) = accept_target.filter(|_| uncertain) {
                        button {
                            class: "btn secondary small",
                            title: "Confirm this match; it then counts as your choice",
                            onclick: move |_| workspace.write().set_table_choice(&accept_key, Some(TableChoice::Target(t.clone()))),
                            Icon { name: "check", size: 15 }
                            "Accept match"
                        }
                    }
                    if by_user {
                        button {
                            class: "btn ghost small",
                            title: "Let the tool decide again",
                            onclick: move |_| workspace.write().set_table_choice(&reset_key, None),
                            Icon { name: "undo", size: 15 }
                            "Reset"
                        }
                    }
                }
            }
            if has_target {
                if !required.is_empty() {
                    div { class: callout_class, role: "note",
                        b { {required_head} }
                        ul {
                            for (name , ty , link) in required {
                                li { key: "{name}",
                                    span { class: "mono", {name.clone()} }
                                    " ({ty}){link}"
                                }
                            }
                        }
                    }
                }
                div { class: "card flush",
                    div { class: "grid-tools",
                        label { class: "search",
                            Icon { name: "search", size: 15 }
                            span { class: "visually-hidden", "Search columns" }
                            input {
                                r#type: "search",
                                placeholder: "Search columns…",
                                value: column_search,
                                oninput: move |e| state.write().columns.text = e.value(),
                            }
                        }
                        div { class: "chip-row", role: "group", aria_label: "Show columns",
                            for (key , label , count , is_pressed , value) in column_chips {
                                button {
                                    key: "{key}",
                                    class: "filter-chip",
                                    aria_pressed: is_pressed,
                                    onclick: move |_| state.write().columns.review = value,
                                    {label}
                                    span { class: "chip-count", {count} }
                                }
                            }
                        }
                    }
                    div { class: "grid-wrap",
                        table { class: "grid",
                            thead {
                                tr {
                                    th { scope: "col", {source_head} }
                                    th { scope: "col", {target_head} }
                                    th { scope: "col", "Type conversion" }
                                    th { scope: "col", class: "result-col", "Result" }
                                    th { scope: "col", span { class: "visually-hidden", "Actions" } }
                                }
                            }
                            tbody {
                                for row in rows {
                                    ColumnRowView { key: "{row.column}", workspace, table: source_key.clone(), row }
                                }
                            }
                        }
                    }
                    if shown == 0 {
                        p { class: "tlist-empty", "No column matches. Clear the search or choose another filter." }
                    } else if shown < rows_total {
                        p { class: "grid-foot muted", "Showing {shown} of {rows_total} columns." }
                    }
                }
                details { class: "card table-sql",
                    summary { "Draft SQL for this table" }
                    div { class: "table-sql-body",
                        p { class: "muted",
                            "For review only; Schema Bridge never runs it. The Draft SQL stage has the whole script in load order."
                        }
                        CopyButton { text: sql.clone(), label: "Copy SQL" }
                        pre { class: "sql", tabindex: "0", code { {sql} } }
                    }
                }
            } else {
                div { class: "callout", role: "note", {message} }
                div { class: "card flush grid-wrap",
                    table { class: "grid",
                        thead {
                            tr {
                                th { scope: "col", "Column" }
                                th { scope: "col", "Type" }
                                th { scope: "col", "Description" }
                            }
                        }
                        tbody {
                            for (name , ty , description) in plain_columns {
                                tr { key: "{name}",
                                    td { class: "mono", {name.clone()} }
                                    td { class: "mono muted", {ty} }
                                    td { {description} }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ColumnRowView(workspace: Signal<Workspace>, table: String, row: RowView) -> Element {
    let mut workspace = workspace;
    let RowView {
        column,
        key_note,
        description,
        source_type,
        options,
        current_target,
        how_class,
        how,
        target_description,
        target_type,
        fit_class,
        fit_label,
        fit_note,
        status_class,
        status,
        notes,
        reason,
        uncertain,
        manual,
    } = row;
    let (t1, t2, t3, t4) = (table.clone(), table.clone(), table.clone(), table);
    let (c1, c2, c3, c4) = (
        column.clone(),
        column.clone(),
        column.clone(),
        column.clone(),
    );
    let row_class = if uncertain {
        "row-uncertain"
    } else if manual {
        "row-manual"
    } else {
        ""
    };
    let mapped = current_target.is_some();
    let types = if target_type.is_empty() {
        String::new()
    } else {
        format!(
            "{} → {}",
            source_type.trim_end_matches(" · required"),
            target_type.trim_end_matches(" · required")
        )
    };
    rsx! {
        tr { class: row_class,
            td {
                span { class: "mono col-name", {column.clone()} }
                if key_note {
                    span { class: "key-tag", "key" }
                }
                p { class: "cell-type mono", {source_type} }
                if !description.is_empty() {
                    p { class: "cell-desc", {description} }
                }
            }
            td {
                label {
                    span { class: "visually-hidden", "Target column for {column}" }
                    select {
                        class: "col-picker",
                        onchange: move |e| {
                            let v = e.value();
                            let choice = match v.as_str() {
                                "@auto" => None,
                                "@none" => Some(None),
                                other => other.strip_prefix("col:").map(|c| Some(c.to_string())),
                            };
                            workspace.write().set_column_choice(&t1, &c1, choice);
                        },
                        for (value , label , sel) in options {
                            option { key: "{value}", value: "{value}", selected: sel, {label} }
                        }
                    }
                }
                if !how.is_empty() {
                    span { class: how_class, {how} }
                }
                if !target_type.is_empty() {
                    p { class: "cell-type mono", {target_type} }
                }
                if !target_description.is_empty() {
                    p { class: "cell-desc", {target_description} }
                }
            }
            td {
                if !fit_label.is_empty() {
                    span { class: fit_class, {fit_label} }
                    p { class: "cell-types mono", {types} }
                    p { class: "cell-desc", {fit_note} }
                } else {
                    span { class: "muted", "—" }
                }
            }
            td {
                span { class: status_class, {status} }
                p { class: "cell-reason", {reason} }
                for (i , (class , text)) in notes.into_iter().enumerate() {
                    p { key: "{i}", class, {text} }
                }
            }
            td { class: "row-actions",
                if uncertain {
                    button {
                        class: "icon-btn accept",
                        title: "Accept this match",
                        aria_label: "Accept the match for {column}",
                        onclick: move |_| {
                            let target = current_target.clone();
                            workspace.write().set_column_choice(&t2, &c2, Some(target));
                        },
                        Icon { name: "check", size: 15 }
                    }
                }
                if mapped {
                    button {
                        class: "icon-btn remove",
                        title: "Remove this mapping (do not copy the column)",
                        aria_label: "Remove the mapping for {column}",
                        onclick: move |_| workspace.write().set_column_choice(&t3, &c3, Some(None)),
                        Icon { name: "x", size: 15 }
                    }
                }
                if manual {
                    button {
                        class: "icon-btn",
                        title: "Let the tool decide again",
                        aria_label: "Reset {column} to the tool's decision",
                        onclick: move |_| workspace.write().set_column_choice(&t4, &c4, None),
                        Icon { name: "undo", size: 15 }
                    }
                }
            }
        }
    }
}

/// What the five type results mean, for users who are not database experts.
#[component]
fn TypeLegend() -> Element {
    rsx! {
        details { class: "card legend-card",
            summary {
                Icon { name: "info", size: 15 }
                "What the type conversion results mean"
            }
            ul { class: "fit-legend",
                for fit in Fit::ALL {
                    li { key: "{fit.code()}",
                        span { class: "fit fit-{fit.code()}", {fit.label()} }
                        span { {fit.meaning()} }
                    }
                }
            }
            p { class: "muted small",
                "Example: timestamp → date is “Potential information loss”, because the time of day is dropped. Text → an ID column is “Requires conversion”: each value must be looked up."
            }
        }
    }
}

/// What happens to every source column, as one bar with a legend. Used on
/// the Matching stage, and on the Load stage with a way to open Matching.
#[component]
pub fn ColumnCoverage(workspace: Signal<Workspace>, open: Option<EventHandler<()>>) -> Element {
    let d = derived();
    let guard = d.analysis.read();
    let ws = workspace.read();
    let (Some(source), Some(analysis)) = (ws.source.schema.as_ref(), guard.as_ref()) else {
        return rsx! {};
    };
    let matching = &analysis.matching;
    let counts = matching.column_counts(source);
    let total: usize = counts.iter().sum();
    let mapped = matching.mappings.len();
    let stays = matching.count(Review::Stays);
    let none = matching.count(Review::Unmatched);
    let (s, t) = (ws.source.name.clone(), ws.target.name.clone());
    let lead = format!(
        "{mapped} of {} {s} tables {} a place in {t}, {stays} {} in {s} on purpose, and {none} {} no place yet.",
        matching.tables.len(),
        if mapped == 1 { "has" } else { "have" },
        if stays == 1 { "stays" } else { "stay" },
        if none == 1 { "has" } else { "have" },
    );
    let heading = format!("What happens to the {total} {s} columns");
    let segments: Vec<(String, String, String)> = ColumnStatus::ALL
        .iter()
        .zip(counts)
        .filter(|(_, n)| *n > 0)
        .map(|(st, n)| {
            (
                format!("seg-bar {}", status_class(*st)),
                format!("width: {:.3}%", 100.0 * n as f64 / total.max(1) as f64),
                format!("{}: {n}", st.meaning()),
            )
        })
        .collect();
    let legend: Vec<(String, String, &str)> = ColumnStatus::ALL
        .iter()
        .zip(counts)
        .map(|(st, n)| {
            (
                format!("swatch {}", status_class(*st)),
                n.to_string(),
                st.meaning(),
            )
        })
        .collect();
    drop(ws);
    drop(guard);

    rsx! {
        section { class: "card coverage",
            div { class: "card-head row",
                div {
                    h3 { {heading} }
                    p { class: "muted", {lead} }
                }
                if let Some(open) = open {
                    button { class: "btn secondary", onclick: move |_| open.call(()), "Review matching" }
                }
            }
            div { class: "bar", role: "img", aria_label: "Share of columns by result",
                for (class , style , title) in segments {
                    span { key: "{class}", class, style, title }
                }
            }
            ul { class: "legend",
                for (class , count , label) in legend {
                    li { key: "{label}",
                        span { class, aria_hidden: "true" }
                        span { class: "legend-count", {count} }
                        {label}
                    }
                }
            }
        }
    }
}

/// The word list: a text editor with a live check of what each line means.
#[component]
fn WordListEditor(workspace: Signal<Workspace>) -> Element {
    let mut workspace = workspace;
    let mut draft = use_signal(|| workspace.read().word_list.clone());
    let saved = workspace.read().word_list.clone();
    let text = draft();
    let parsed = WordList::parse(&text);
    let changed = text != saved;
    let summary = format!(
        "{} words · {} column names · {} table pairs · {} column pairs · {} skip patterns",
        parsed.words.len(),
        parsed.column_names.len(),
        parsed.tables.len(),
        parsed.columns.len(),
        parsed.skip.len()
    );
    let problems: Vec<String> = parsed
        .unrecognised
        .iter()
        .map(|(n, line)| format!("Line {n} is not a rule and is ignored: {line}"))
        .collect();
    let state = if changed {
        "Not applied yet. Matching uses the word list after you press Apply."
    } else {
        "Applied. Matching uses this word list."
    };

    rsx! {
        div { class: "words-layout",
            section { class: "card words-editor",
                label { class: "field-label", r#for: "word-list", "Word list" }
                textarea {
                    id: "word-list",
                    spellcheck: "false",
                    value: text,
                    oninput: move |e| draft.set(e.value()),
                }
                p { class: "muted words-summary", {summary} }
                for (i , p) in problems.into_iter().enumerate() {
                    p { key: "{i}", class: "notice error", role: "alert", {p} }
                }
                div { class: "words-actions",
                    span { class: if changed { "words-state pending" } else { "words-state" }, role: "status", {state} }
                    label { class: "btn ghost", r#for: "word-list-file", title: "Read a word list from a text file (apply it afterwards)", "Load from file…" }
                    input {
                        id: "word-list-file",
                        class: "visually-hidden",
                        r#type: "file",
                        accept: ".txt,text/plain",
                        onchange: move |evt| async move {
                            if let Some(file) = evt.files().into_iter().next() {
                                if let Ok(text) = file.read_string().await {
                                    draft.set(text);
                                }
                            }
                        },
                    }
                    button {
                        class: "btn ghost",
                        title: "Save the word list as it is in the editor",
                        onclick: move |_| download_text("word-list.txt", "text/plain", draft()),
                        "Save to file"
                    }
                    button {
                        class: "btn ghost",
                        disabled: !changed,
                        onclick: move |_| draft.set(workspace.read().word_list.clone()),
                        "Discard changes"
                    }
                    button {
                        class: "btn primary",
                        disabled: !changed,
                        onclick: move |_| {
                            let text = draft();
                            workspace.write().word_list = text;
                        },
                        "Apply word list"
                    }
                }
            }
            aside { class: "card words-help",
                h3 { "What the word list does" }
                p { "Tables and columns are matched by their names. Two teams often use different words for the same thing; the word list teaches Schema Bridge which ones mean the same." }
                dl { class: "rules",
                    dt { class: "mono", "component = device" }
                    dd { "A word in the source means a word in the target." }
                    dt { class: "mono", "limit_min = num_min_value" }
                    dd { "A whole column name means another column name (a name with _)." }
                    dt { class: "mono", "table: run = session" }
                    dd {
                        "This table goes into that table. Use "
                        span { class: "mono", "none" }
                        " when it has no place yet."
                    }
                    dt { class: "mono", "column: machine.serial_no = asset.code" }
                    dd { "This column goes into that column." }
                    dt { class: "mono", "skip: timetable*, user_*" }
                    dd {
                        "These tables stay in the source. "
                        span { class: "mono", "*" }
                        " matches anything."
                    }
                }
                p { class: "muted", "Lines starting with # are notes. Choices you make on the Tables and columns view override the word list." }
            }
        }
    }
}
