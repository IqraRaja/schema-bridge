//! The comparison report: a summary of the whole comparison for someone who
//! did not run it, the integration plan's six steps, and the full report as
//! Markdown or as a self-contained HTML page.
//!
//! The report is built once as a list of [`Block`]s and rendered to either
//! format, so both always say the same.

use crate::checks::{Finding, Severity, count_by_severity};
use crate::export::{all_findings, origin_label};
use crate::influx::retention_summary;
use crate::matching::{ColumnStatus, MatchKind, Review};
use crate::model::{Schema, TimeSeriesStats, table_name};
use crate::types::Fit;
use crate::workspace::{Analysis, Side, Workspace, grouped, system_name};

/// Tables of the source, by what happens to them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TableCounts {
    pub total: usize,
    pub matched: usize,
    pub uncertain: usize,
    pub manual: usize,
    pub no_place: usize,
    pub stays: usize,
}

/// Columns of the source, by what happens to them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ColumnCounts {
    pub total: usize,
    /// Copied into a target column.
    pub mapped: usize,
    /// Not copied (no place in the target, or removed by hand).
    pub not_mapped: usize,
    /// Passwords and secrets, and columns of tables that stay in the source.
    pub never_copied: usize,
    /// Mapped columns by type fit, in [`Fit::ALL`] order.
    pub by_fit: [usize; 5],
    pub uncertain: usize,
    pub manual: usize,
}

impl ColumnCounts {
    /// Mapped columns that need an explicit conversion.
    pub fn requiring_conversion(&self) -> usize {
        self.by_fit[Fit::Convert as usize]
    }
    pub fn with_possible_loss(&self) -> usize {
        self.by_fit[Fit::Loss as usize]
    }
    pub fn incompatible(&self) -> usize {
        self.by_fit[Fit::Incompatible as usize]
    }
}

/// Something someone still has to decide or check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unresolved {
    /// `Fix first`, `Decide`, `Review` or `Draft SQL`.
    pub kind: &'static str,
    pub item: String,
    pub detail: String,
}

/// The whole comparison in numbers and lists.
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub source: String,
    pub target: String,
    /// What the source's objects are called: "tables" or "measurements".
    pub source_noun: &'static str,
    pub tables: TableCounts,
    pub columns: ColumnCounts,
    /// Findings per origin ("Workshop", "Catalogue", "Fitting them
    /// together"), counted by severity in [`Severity::ALL`] order.
    pub problems: Vec<(String, [usize; 5])>,
    /// Blocking and high problems in fitting the two together.
    pub risks: Vec<Finding>,
    pub unresolved: Vec<Unresolved>,
    /// `TODO` markers in the draft SQL.
    pub todos: usize,
}

impl Summary {
    pub fn total_problems(&self, severity: Severity) -> usize {
        self.problems
            .iter()
            .map(|(_, c)| c[severity as usize])
            .sum()
    }
}

/// The six steps of the integration plan (specification §8).
#[derive(Debug, Clone, PartialEq)]
pub struct PlanSteps {
    /// 1. Blocking and high problems inside the target.
    pub fix_target: Vec<Finding>,
    /// 2. Blocking and high problems inside the source.
    pub check_source: Vec<Finding>,
    /// 3. Blocking and high problems in fitting them together …
    pub decisions: Vec<Finding>,
    /// … and the source tables with no place in the target.
    pub no_place: Vec<String>,
    /// 4. Target tables in load order: (table, loaded from, on a cycle).
    pub load_order: Vec<(String, Vec<String>, bool)>,
    /// 5. Open TODOs in the draft SQL.
    pub todos: usize,
    /// 6. Unchecked links in the source to count before loading.
    pub unchecked_links: Vec<String>,
}

fn serious(f: &Finding) -> bool {
    f.severity <= Severity::High
}

/// Works out the plan's steps for an analysis of the workspace.
pub fn plan_steps(ws: &Workspace, analysis: &Analysis) -> PlanSteps {
    let findings = ws.findings();
    let side = |s: Side| -> Vec<Finding> {
        findings
            .iter()
            .filter(|(x, f)| *x == s && serious(f))
            .map(|(_, f)| f.clone())
            .collect()
    };
    PlanSteps {
        fix_target: side(Side::Target),
        check_source: side(Side::Source),
        decisions: analysis
            .integration
            .iter()
            .filter(|f| serious(f))
            .cloned()
            .collect(),
        no_place: analysis
            .matching
            .tables
            .iter()
            .filter(|m| m.kind == MatchKind::NoTarget)
            .map(|m| table_name(&m.source).to_string())
            .collect(),
        load_order: analysis
            .load_order
            .iter()
            .map(|s| (table_name(&s.table).to_string(), s.from.clone(), s.cycle))
            .collect(),
        todos: ws.draft_sql(analysis).matches("TODO").count(),
        unchecked_links: findings
            .iter()
            .filter(|(s, f)| *s == Side::Source && f.rule == "LINK-IMPLICIT")
            .map(|(_, f)| f.object.clone())
            .collect(),
    }
}

/// Counts and lists for the comparison report. Without both databases the
/// matching parts are empty.
pub fn summary(ws: &Workspace, analysis: Option<&Analysis>) -> Summary {
    let mut tables = TableCounts::default();
    let mut columns = ColumnCounts::default();
    let mut unresolved = Vec::new();
    let all = all_findings(ws, analysis);

    for (side, f) in &all {
        if f.severity == Severity::Blocking && side.is_some() {
            unresolved.push(Unresolved {
                kind: "Fix first",
                item: format!("{}: {}", origin_label(ws, *side), f.title),
                detail: f.object.clone(),
            });
        }
    }

    let mut todos = 0;
    if let (Some(a), Some(source)) = (analysis, ws.source.schema.as_ref()) {
        let m = &a.matching;
        tables = TableCounts {
            total: m.tables.len(),
            matched: m.count(Review::Matched),
            uncertain: m.count(Review::Uncertain),
            manual: m.count(Review::Manual),
            no_place: m.count(Review::Unmatched),
            stays: m.count(Review::Stays),
        };
        let counts = m.column_counts(source);
        columns.total = counts.iter().sum();
        columns.never_copied = counts[ColumnStatus::NeverCopy as usize];
        columns.not_mapped = counts[ColumnStatus::NotCopied as usize];
        for p in &m.mappings {
            for r in &p.rows {
                if let Some(c) = &r.compatibility {
                    columns.mapped += 1;
                    columns.by_fit[c.fit as usize] += 1;
                }
                columns.uncertain += usize::from(r.is_uncertain());
                columns.manual += usize::from(r.manual);
            }
            for r in &p.required {
                let link = r
                    .link_to
                    .as_deref()
                    .map(|k| format!(" (a link to {})", table_name(k)))
                    .unwrap_or_default();
                unresolved.push(Unresolved {
                    kind: "Decide",
                    item: format!("{}.{}", table_name(&p.target), r.column.name),
                    detail: format!(
                        "Required{link}, but nothing in {} fills it.",
                        table_name(&p.source)
                    ),
                });
            }
        }
        for t in &m.tables {
            if t.kind == MatchKind::NoTarget {
                unresolved.push(Unresolved {
                    kind: "Decide",
                    item: table_name(&t.source).to_string(),
                    detail: format!("No place in {} yet.", ws.target.name),
                });
            }
            if t.is_uncertain() {
                unresolved.push(Unresolved {
                    kind: "Review",
                    item: format!(
                        "{} → {}",
                        table_name(&t.source),
                        t.target.as_deref().map(table_name).unwrap_or("–")
                    ),
                    detail: t.reason(),
                });
            }
        }
        for p in &m.mappings {
            for r in p.rows.iter().filter(|r| r.is_uncertain()) {
                unresolved.push(Unresolved {
                    kind: "Review",
                    item: format!(
                        "{}.{} → {}.{}",
                        table_name(&p.source),
                        r.source.name,
                        table_name(&p.target),
                        r.target.as_ref().map_or("", |c| c.name.as_str())
                    ),
                    detail: r.reason(),
                });
            }
        }
        todos = ws.draft_sql(a).matches("TODO").count();
        if todos > 0 {
            unresolved.push(Unresolved {
                kind: "Draft SQL",
                item: format!("{todos} TODO markers"),
                detail: "Each marks a decision the draft could not make; resolve them before a test load.".into(),
            });
        }
    }

    let mut problems = Vec::new();
    let mut origins: Vec<Option<Side>> = [Side::Source, Side::Target]
        .into_iter()
        .filter(|s| ws.slot(*s).schema.is_some())
        .map(Some)
        .collect();
    if analysis.is_some() {
        origins.push(None);
    }
    for o in origins {
        let counts = count_by_severity(all.iter().filter(|(s, _)| *s == o).map(|(_, f)| f));
        problems.push((origin_label(ws, o), counts));
    }

    Summary {
        source: ws.source.name.clone(),
        target: ws.target.name.clone(),
        source_noun: ws
            .source
            .schema
            .as_ref()
            .map_or("tables", |x| x.object_noun()),
        tables,
        columns,
        problems,
        risks: analysis
            .map(|a| {
                a.integration
                    .iter()
                    .filter(|f| serious(f))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default(),
        unresolved,
        todos,
    }
}

/// One part of the report document.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading(u8, String),
    Paragraph(String),
    /// A highlighted remark (a warning or an important assumption).
    Note(String),
    List(Vec<String>),
    Table {
        head: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Code(String),
}

fn heading(level: u8, text: impl Into<String>) -> Block {
    Block::Heading(level, text.into())
}
fn para(text: impl Into<String>) -> Block {
    Block::Paragraph(text.into())
}
fn table(head: &[&str], rows: Vec<Vec<String>>) -> Block {
    Block::Table {
        head: head.iter().map(|h| h.to_string()).collect(),
        rows,
    }
}

fn type_of(label: &str, data_type: &str) -> String {
    if label.is_empty() { data_type } else { label }.to_string()
}

/// The full comparison report as a document.
pub fn report_blocks(ws: &Workspace, analysis: Option<&Analysis>) -> Vec<Block> {
    let s = summary(ws, analysis);
    let (sn, tn) = (&ws.source.name, &ws.target.name);
    let mut out = vec![
        heading(1, format!("Schema Bridge report: {sn} → {tn}")),
        para(
            "This report compares the structure of two databases and plans how to bring the source into the target. \
             It was produced from structure exports only: no table contents were read and no database was connected. \
             Findings are risks seen in the structure; confirm them on the data before acting.",
        ),
    ];
    if ws.using_samples {
        out.push(Block::Note(
            "These are the made-up sample schemas shipped with Schema Bridge, not real databases."
                .into(),
        ));
    }

    // Summary.
    out.push(heading(2, "Summary"));
    if analysis.is_some() {
        let source_noun = s.source_noun;
        let t = s.tables;
        let c = s.columns;
        out.push(table(
            &["Tables", "Count"],
            vec![
                vec![format!("Source {}", source_noun), t.total.to_string()],
                vec!["Matched to a target table".into(), t.matched.to_string()],
                vec!["Uncertain (please review)".into(), t.uncertain.to_string()],
                vec!["Changed by hand".into(), t.manual.to_string()],
                vec!["No place in the target yet".into(), t.no_place.to_string()],
                vec!["Stay in the source on purpose".into(), t.stays.to_string()],
            ],
        ));
        out.push(table(
            &["Columns", "Count"],
            vec![
                vec!["Source columns".into(), c.total.to_string()],
                vec!["Mapped to a target column".into(), c.mapped.to_string()],
                vec![
                    "… same type or safe".into(),
                    (c.by_fit[0] + c.by_fit[1]).to_string(),
                ],
                vec![
                    "… requiring conversion".into(),
                    c.requiring_conversion().to_string(),
                ],
                vec![
                    "… potential information loss".into(),
                    c.with_possible_loss().to_string(),
                ],
                vec!["… incompatible".into(), c.incompatible().to_string()],
                vec!["Not mapped".into(), c.not_mapped.to_string()],
                vec![
                    "Never copied (secrets, tables that stay)".into(),
                    c.never_copied.to_string(),
                ],
                vec!["Uncertain matches".into(), c.uncertain.to_string()],
                vec!["Changed by hand".into(), c.manual.to_string()],
            ],
        ));
    }
    let mut rows = Vec::new();
    for (origin, counts) in &s.problems {
        let mut r = vec![origin.clone()];
        r.extend(counts.iter().map(|n| n.to_string()));
        rows.push(r);
    }
    out.push(table(
        &["Findings", "Blocking", "High", "Medium", "Low", "Info"],
        rows,
    ));

    // Databases.
    out.push(heading(2, "Databases"));
    let stats = [Side::Source, Side::Target].map(|side| {
        ws.slot(side)
            .schema
            .as_ref()
            .map(|x| (system_name(&x.dbms).to_string(), x.stats()))
    });
    let cell = |i: usize, f: fn(&crate::model::SchemaStats) -> i64| {
        stats[i]
            .as_ref()
            .map_or("–".to_string(), |(_, st)| grouped(f(st)))
    };
    let system = |i: usize| {
        stats[i]
            .as_ref()
            .map_or("not loaded".to_string(), |(n, _)| n.clone())
    };
    out.push(Block::Table {
        head: vec![String::new(), sn.clone(), tn.clone()],
        rows: vec![
            vec!["System".into(), system(0), system(1)],
            vec![
                "Tables".into(),
                cell(0, |s| s.tables as i64),
                cell(1, |s| s.tables as i64),
            ],
            vec![
                "Columns".into(),
                cell(0, |s| s.columns as i64),
                cell(1, |s| s.columns as i64),
            ],
            vec![
                "Links (foreign keys)".into(),
                cell(0, |s| s.links as i64),
                cell(1, |s| s.links as i64),
            ],
            vec![
                "Tables with a primary key".into(),
                cell(0, |s| s.tables_with_primary_key as i64),
                cell(1, |s| s.tables_with_primary_key as i64),
            ],
            vec![
                "Rows (estimated)".into(),
                cell(0, |s| s.estimated_rows),
                cell(1, |s| s.estimated_rows),
            ],
        ],
    });
    let schemas = [&ws.source.schema, &ws.target.schema];
    if schemas
        .iter()
        .any(|x| x.as_ref().is_some_and(Schema::is_time_series))
    {
        let ts = |i: usize, f: fn(&TimeSeriesStats) -> Option<i64>| {
            schemas[i]
                .as_ref()
                .filter(|x| x.is_time_series())
                .and_then(|x| f(&x.time_series_stats()))
                .map_or("–".to_string(), grouped)
        };
        let retention = |i: usize| {
            schemas[i]
                .as_ref()
                .and_then(retention_summary)
                .unwrap_or_else(|| "–".to_string())
        };
        out.push(Block::Table {
            head: vec!["Time series".into(), sn.clone(), tn.clone()],
            rows: vec![
                vec![
                    "Measurements".into(),
                    ts(0, |s| Some(s.measurements as i64)),
                    ts(1, |s| Some(s.measurements as i64)),
                ],
                vec![
                    "Tags".into(),
                    ts(0, |s| Some(s.tags as i64)),
                    ts(1, |s| Some(s.tags as i64)),
                ],
                vec![
                    "Fields".into(),
                    ts(0, |s| Some(s.fields as i64)),
                    ts(1, |s| Some(s.fields as i64)),
                ],
                vec!["Series".into(), ts(0, |s| s.series), ts(1, |s| s.series)],
                vec!["Retention".into(), retention(0), retention(1)],
            ],
        });
    }

    // Unresolved items and risks.
    out.push(heading(2, "Unresolved items"));
    if s.unresolved.is_empty() {
        out.push(para("Nothing is left open."));
    } else {
        out.push(table(
            &["Kind", "Item", "Detail"],
            s.unresolved
                .iter()
                .map(|u| vec![u.kind.to_string(), u.item.clone(), u.detail.clone()])
                .collect(),
        ));
    }
    if analysis.is_some() {
        out.push(heading(2, "Integration risks"));
        if s.risks.is_empty() {
            out.push(para("No blocking or high integration issues."));
        } else {
            out.push(Block::List(
                s.risks
                    .iter()
                    .map(|f| {
                        format!(
                            "{}: {} ({}). What to do: {}",
                            f.severity.label(),
                            f.title,
                            f.object,
                            f.fix
                        )
                    })
                    .collect(),
            ));
        }
    }

    // Problems.
    out.push(heading(2, "All findings"));
    let all = all_findings(ws, analysis);
    if all.is_empty() {
        out.push(para("No findings."));
    } else {
        out.push(table(
            &[
                "Severity",
                "Database",
                "Rule",
                "Object",
                "Finding",
                "What to do",
            ],
            all.iter()
                .map(|(side, f)| {
                    vec![
                        f.severity.label().to_string(),
                        origin_label(ws, *side),
                        f.rule.to_string(),
                        f.object.clone(),
                        format!("{} {}", f.title, f.explanation),
                        f.fix.clone(),
                    ]
                })
                .collect(),
        ));
    }

    let Some(a) = analysis else {
        out.push(Block::Note(
            "Load both databases to add matching, the integration plan and the draft SQL to this report.".into(),
        ));
        return out;
    };

    // Mapping.
    out.push(heading(2, "Mapping"));
    out.push(table(
        &["Source table", "Target table", "Decision"],
        a.matching
            .tables
            .iter()
            .map(|m| {
                vec![
                    table_name(&m.source).to_string(),
                    m.target
                        .as_deref()
                        .map_or("–".into(), |t| table_name(t).to_string()),
                    m.reason(),
                ]
            })
            .collect(),
    ));
    for p in &a.matching.mappings {
        out.push(heading(
            3,
            format!("{} → {}", table_name(&p.source), table_name(&p.target)),
        ));
        out.push(table(
            &["Source column", "Target column", "Type fit", "Why", "Notes"],
            p.rows
                .iter()
                .map(|r| {
                    let source = format!(
                        "{} ({})",
                        r.source.name,
                        type_of(&r.source.type_label, &r.source.data_type)
                    );
                    let target = r.target.as_ref().map_or("not copied".to_string(), |c| {
                        format!("{} ({})", c.name, type_of(&c.type_label, &c.data_type))
                    });
                    let fit = r
                        .compatibility
                        .as_ref()
                        .map_or(String::new(), |c| format!("{}: {}", c.fit.label(), c.note));
                    let mut why = r.reason();
                    if let Some(e) = r
                        .evidence
                        .filter(|_| r.how == Some(crate::matching::ColumnHow::Automatic))
                    {
                        why.push_str(&format!(" Confidence {:.0}%.", e.confidence() * 100.0));
                    }
                    let notes: Vec<&str> = r
                        .notes
                        .iter()
                        .filter(|n| n.kind != crate::matching::NoteKind::Type)
                        .map(|n| n.text.as_str())
                        .collect();
                    vec![source, target, fit, why, notes.join(" ")]
                })
                .collect(),
        ));
        if !p.required.is_empty() {
            out.push(Block::Note(format!(
                "{} needs values that the source does not have: {}.",
                table_name(&p.target),
                p.required
                    .iter()
                    .map(|r| r.column.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    }

    // Manual overrides.
    out.push(heading(2, "Changes made by hand"));
    let mut manual = Vec::new();
    for m in a
        .matching
        .tables
        .iter()
        .filter(|m| m.decided_by == crate::matching::DecidedBy::User)
    {
        manual.push(format!("Table {}: {}", table_name(&m.source), m.reason()));
    }
    for p in &a.matching.mappings {
        for r in p.rows.iter().filter(|r| r.manual) {
            manual.push(match &r.target {
                Some(c) => format!(
                    "Column {}.{} → {}.{}",
                    table_name(&p.source),
                    r.source.name,
                    table_name(&p.target),
                    c.name
                ),
                None => format!(
                    "Column {}.{}: not copied",
                    table_name(&p.source),
                    r.source.name
                ),
            });
        }
    }
    if manual.is_empty() {
        out.push(para(
            "None: every match was made by the word list or automatically.",
        ));
    } else {
        out.push(Block::List(manual));
    }

    // Type conversions.
    out.push(heading(2, "Type conversions"));
    out.push(para(
        "Mapped columns whose types differ. Same: copied unchanged. Safe: fits without loss. Requires conversion: \
         converted or looked up on the way. Potential information loss: values may be cut, rounded or lose detail. \
         Incompatible: someone must decide.",
    ));
    let mut conversions = Vec::new();
    for p in &a.matching.mappings {
        for r in &p.rows {
            if let (Some(tc), Some(c)) = (&r.target, &r.compatibility) {
                if c.fit != Fit::Same {
                    conversions.push(vec![
                        format!(
                            "{}.{} ({}) → {}.{} ({})",
                            table_name(&p.source),
                            r.source.name,
                            type_of(&r.source.type_label, &r.source.data_type),
                            table_name(&p.target),
                            tc.name,
                            type_of(&tc.type_label, &tc.data_type)
                        ),
                        c.fit.label().to_string(),
                        c.note.clone(),
                    ]);
                }
            }
        }
    }
    if conversions.is_empty() {
        out.push(para("Every mapped column has the same type on both sides."));
    } else {
        out.push(table(&["Column", "Result", "Explanation"], conversions));
    }

    // Integration plan.
    let plan = plan_steps(ws, a);
    out.push(heading(2, "Integration plan"));
    let titles = |list: &[Finding]| -> Vec<String> {
        list.iter()
            .map(|f| format!("{}: {} ({})", f.severity.label(), f.title, f.object))
            .collect()
    };
    let step = |out: &mut Vec<Block>, n: usize, title: String, items: Vec<String>, none: &str| {
        out.push(heading(3, format!("{n}. {title}")));
        if items.is_empty() {
            out.push(para(none));
        } else {
            out.push(Block::List(items));
        }
    };
    step(
        &mut out,
        1,
        format!("Fix the schema findings in {tn}"),
        titles(&plan.fix_target),
        "Nothing serious found.",
    );
    step(
        &mut out,
        2,
        format!("Check the data in {sn}"),
        titles(&plan.check_source),
        "Nothing serious found.",
    );
    let mut decisions = titles(&plan.decisions);
    if !plan.no_place.is_empty() {
        decisions.push(format!("No place yet for: {}", plan.no_place.join(", ")));
    }
    step(
        &mut out,
        3,
        "Make the decisions".into(),
        decisions,
        "No open decisions.",
    );
    let order = plan
        .load_order
        .iter()
        .map(|(t, from, cycle)| {
            let from = if from.is_empty() {
                "not loaded; must already contain the rows other tables link to".to_string()
            } else {
                format!("from {}", from.join(", "))
            };
            format!(
                "{t}: {from}{}",
                if *cycle {
                    " (part of a cycle: load in two passes)"
                } else {
                    ""
                }
            )
        })
        .collect();
    step(
        &mut out,
        4,
        "Load the tables in this order".into(),
        order,
        "No table goes into the target yet.",
    );
    if ws.sql_available() {
        step(
            &mut out,
            5,
            "Run a test load with the draft SQL".into(),
            vec![format!(
                "The draft below has {} TODO marker(s). Run it only on a test copy, inside its transaction.",
                plan.todos
            )],
            "",
        );
    } else {
        step(
            &mut out,
            5,
            "Run a test load with the target's own tools".into(),
            vec![format!(
                "{tn} is a time-series database. Schema Bridge drafts no SQL and no line protocol for it; write a test load with its own tools, using the mapping."
            )],
            "",
        );
    }
    let mut checks = vec![
        "Row counts: every source table and its target table have the same number of rows (minus rows filtered on purpose).".to_string(),
        format!("Links: every copied link points to a real record in {tn}."),
        "Run the load a second time: nothing should change.".into(),
        "Compare three or four records in both databases by eye.".into(),
    ];
    if !plan.unchecked_links.is_empty() {
        checks.push(format!(
            "Count values of unchecked links in {sn} first: {}.",
            plan.unchecked_links.join(", ")
        ));
    }
    step(&mut out, 6, "Check the result".into(), checks, "");

    // Draft SQL.
    out.push(heading(2, "Draft SQL"));
    if !ws.sql_available() {
        out.push(Block::Note(format!(
            "Not available: {tn} is a time-series database. Time-series databases are not written with SQL INSERT statements, \
             and Schema Bridge does not generate InfluxDB line protocol or other write operations."
        )));
    } else {
        out.push(Block::Note(
        "A draft for review, in PostgreSQL syntax. Schema Bridge never runs it. Run it only on a test copy of the target; \
         it opens a transaction and leaves COMMIT commented out. Connection options are placeholders."
            .into(),
    ));
        out.push(Block::Code(ws.draft_sql(a)));
    }

    // Assumptions.
    out.push(heading(2, "Assumptions and limitations"));
    out.push(Block::List(vec![
        "Only the structure was compared: table and column names, types, keys, links, indexes and descriptions. Views, triggers, procedures, check constraint contents, permissions and partitioning were not compared.".into(),
        "Row counts are the databases' own estimates and may be out of date.".into(),
        "Matches come from names, the word list, descriptions and types; they are suggestions to review, not facts.".into(),
        "Times without a time zone are assumed to need one when the target stores it; the draft asks which zone they were recorded in.".into(),
        "The draft SQL is written for a PostgreSQL target; for other systems it is a column-by-column guide.".into(),
    ]));
    if [&ws.source.schema, &ws.target.schema]
        .iter()
        .any(|x| x.as_ref().is_some_and(Schema::is_time_series))
    {
        out.push(Block::List(vec![
            "Time-series schemas come from a schema export (measurements, tags, fields, field types, series counts and retention), not from the data. A tag combination plus the time identifies a point in InfluxDB; Schema Bridge does not treat it as a primary key and does not add one.".into(),
            "InfluxDB times have nanosecond precision; relational time types keep microseconds or less.".into(),
            "Retention is shown as exported. A relational target has no equivalent setting, so old rows stay until deleted.".into(),
        ]));
    }
    out
}

/// The report as Markdown.
pub fn report_markdown(ws: &Workspace, analysis: Option<&Analysis>) -> String {
    render_markdown(&report_blocks(ws, analysis))
}

/// The report as one self-contained HTML page (no external files).
pub fn report_html(ws: &Workspace, analysis: Option<&Analysis>) -> String {
    let title = format!(
        "Schema Bridge report: {} → {}",
        ws.source.name, ws.target.name
    );
    render_html(&title, &report_blocks(ws, analysis))
}

fn md_cell(text: &str) -> String {
    text.replace('|', "\\|").replace(['\n', '\r'], " ")
}

pub fn render_markdown(blocks: &[Block]) -> String {
    let mut out = String::new();
    for b in blocks {
        match b {
            Block::Heading(level, text) => {
                out.push_str(&format!("{} {text}\n\n", "#".repeat(*level as usize)))
            }
            Block::Paragraph(text) => out.push_str(&format!("{text}\n\n")),
            Block::Note(text) => out.push_str(&format!("> **Note:** {text}\n\n")),
            Block::List(items) => {
                for i in items {
                    out.push_str(&format!("- {}\n", i.replace('\n', " ")));
                }
                out.push('\n');
            }
            Block::Table { head, rows } => {
                let h: Vec<String> = head.iter().map(|c| md_cell(c)).collect();
                out.push_str(&format!("| {} |\n", h.join(" | ")));
                out.push_str(&format!("|{}\n", " --- |".repeat(head.len())));
                for r in rows {
                    let cells: Vec<String> = r.iter().map(|c| md_cell(c)).collect();
                    out.push_str(&format!("| {} |\n", cells.join(" | ")));
                }
                out.push('\n');
            }
            Block::Code(text) => out.push_str(&format!("```sql\n{}\n```\n\n", text.trim_end())),
        }
    }
    out
}

/// Escapes text for HTML.
fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

const REPORT_CSS: &str = "body{font:15px/1.55 -apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,Arial,sans-serif;color:#111827;background:#fff;margin:0}\
main{max-width:1040px;margin:0 auto;padding:40px 24px 64px}\
h1{font-size:26px;margin:0 0 12px;letter-spacing:-.01em}h2{font-size:19px;margin:36px 0 10px;padding-top:14px;border-top:1px solid #e5e7eb}h3{font-size:15.5px;margin:22px 0 8px}\
p{margin:0 0 12px;color:#374151;max-width:80ch}\
.note{border-left:4px solid #a15c07;background:#fdf3e0;padding:10px 14px;border-radius:6px;margin:0 0 14px;color:#4b3500}\
table{border-collapse:collapse;width:100%;margin:0 0 16px;font-size:13.5px}th,td{border:1px solid #e5e7eb;padding:6px 9px;text-align:left;vertical-align:top}th{background:#f3f4f6;font-weight:650}\
tr:nth-child(even) td{background:#fafafa}ul{margin:0 0 14px;padding-left:22px}li{margin:3px 0}\
pre{background:#f6f8fa;border:1px solid #e5e7eb;border-radius:6px;padding:12px 14px;overflow:auto;font:12.5px/1.6 ui-monospace,Menlo,Consolas,monospace;white-space:pre-wrap}\
footer{margin-top:40px;color:#6b7280;font-size:12.5px}\
@media print{main{padding:0}h2{break-after:avoid}table,pre{break-inside:auto}}";

pub fn render_html(title: &str, blocks: &[Block]) -> String {
    let mut body = String::new();
    for b in blocks {
        match b {
            Block::Heading(level, text) => {
                let l = (*level).clamp(1, 6);
                body.push_str(&format!("<h{l}>{}</h{l}>\n", esc(text)));
            }
            Block::Paragraph(text) => body.push_str(&format!("<p>{}</p>\n", esc(text))),
            Block::Note(text) => body.push_str(&format!("<p class=\"note\">{}</p>\n", esc(text))),
            Block::List(items) => {
                body.push_str("<ul>\n");
                for i in items {
                    body.push_str(&format!("<li>{}</li>\n", esc(i)));
                }
                body.push_str("</ul>\n");
            }
            Block::Table { head, rows } => {
                body.push_str("<table>\n<thead><tr>");
                for h in head {
                    body.push_str(&format!("<th>{}</th>", esc(h)));
                }
                body.push_str("</tr></thead>\n<tbody>\n");
                for r in rows {
                    body.push_str("<tr>");
                    for c in r {
                        body.push_str(&format!("<td>{}</td>", esc(c)));
                    }
                    body.push_str("</tr>\n");
                }
                body.push_str("</tbody>\n</table>\n");
            }
            Block::Code(text) => body.push_str(&format!("<pre><code>{}</code></pre>\n", esc(text))),
        }
    }
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{}</title>\n<style>{REPORT_CSS}</style>\n</head>\n<body>\n<main>\n{body}<footer>Made with Schema Bridge from structure exports. No table contents were read.</footer>\n</main>\n</body>\n</html>\n",
        esc(title)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matching::TableChoice;

    fn sample() -> (Workspace, Analysis) {
        let mut ws = Workspace::new();
        ws.load_samples();
        let a = ws.analysis().unwrap();
        (ws, a)
    }

    #[test]
    fn the_summary_counts_tables_columns_and_problems() {
        let (ws, a) = sample();
        let s = summary(&ws, Some(&a));
        assert_eq!(
            s.tables,
            TableCounts {
                total: 5,
                matched: 4,
                uncertain: 4,
                manual: 0,
                no_place: 0,
                stays: 1
            }
        );
        assert_eq!(s.columns.total, 29);
        assert_eq!(
            s.columns.mapped + s.columns.not_mapped + s.columns.never_copied,
            29
        );
        assert_eq!(s.columns.by_fit.iter().sum::<usize>(), s.columns.mapped);
        // Four id → uuid mappings are incompatible; two times need a time zone.
        assert_eq!(s.columns.incompatible(), 4);
        assert_eq!(s.columns.requiring_conversion(), 2);
        assert_eq!(s.columns.uncertain, 4);
        assert_eq!(s.total_problems(Severity::Blocking), 2);
        assert_eq!(s.problems.len(), 3);
        assert_eq!(s.risks.len(), 4);
        assert_eq!(s.todos, 9);
        // The open decisions: the required link, both name columns, reviews, the TODOs.
        let kinds = |k: &str| s.unresolved.iter().filter(|u| u.kind == k).count();
        assert_eq!(kinds("Fix first"), 1);
        assert_eq!(kinds("Decide"), 3);
        assert_eq!(kinds("Review"), 5); // operator → person, four id columns
        assert_eq!(kinds("Draft SQL"), 1);
        assert!(s.unresolved.iter().any(|u| u.item == "asset.asset_type_id"));
    }

    #[test]
    fn choices_made_by_hand_change_the_summary() {
        let (mut ws, _) = sample();
        ws.set_table_choice("main.operator", Some(TableChoice::NoTarget));
        ws.set_column_choice("main.run", "id", Some(None));
        let a = ws.analysis().unwrap();
        let s = summary(&ws, Some(&a));
        assert_eq!(s.tables.no_place, 1);
        assert_eq!(s.tables.manual, 2);
        assert_eq!(s.columns.manual, 1);
        assert!(
            s.unresolved
                .iter()
                .any(|u| u.item == "operator" && u.kind == "Decide")
        );
    }

    #[test]
    fn the_plan_has_its_six_steps() {
        let (ws, a) = sample();
        let p = plan_steps(&ws, &a);
        assert_eq!(p.fix_target.len(), 2);
        assert!(p.check_source.is_empty());
        assert_eq!(p.decisions.len(), 4);
        assert!(p.no_place.is_empty());
        assert_eq!(p.load_order[0], ("asset_type".to_string(), vec![], false));
        assert_eq!(p.load_order.len(), 5);
        assert_eq!(p.todos, 9);
        assert_eq!(p.unchecked_links, ["machine_function.machine_id"]);
    }

    #[test]
    fn the_report_has_every_section() {
        let (ws, a) = sample();
        let md = report_markdown(&ws, Some(&a));
        for h in [
            "# Schema Bridge report: Workshop (sample) → Catalogue (sample)",
            "## Summary",
            "## Databases",
            "## Unresolved items",
            "## Integration risks",
            "## All findings",
            "## Mapping",
            "### machine → asset",
            "## Changes made by hand",
            "## Type conversions",
            "## Integration plan",
            "### 4. Load the tables in this order",
            "## Draft SQL",
            "```sql",
            "## Assumptions and limitations",
        ] {
            assert!(md.contains(h), "missing {h}");
        }
        assert!(md.contains("Potential information loss") || md.contains("Incompatible"));
        assert!(md.contains("-- COMMIT;"));
        assert!(md.contains("made-up sample schemas"));
    }

    #[test]
    fn the_html_report_is_self_contained_and_escaped() {
        let (mut ws, _) = sample();
        ws.source.name = "<script>alert(1)</script> & co".into();
        let a = ws.analysis().unwrap();
        let html = report_html(&ws, Some(&a));
        assert!(html.starts_with("<!doctype html>"));
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt; &amp; co"));
        // No external resources.
        assert!(!html.contains("http://") && !html.contains("https://") && !html.contains("<link"));
        let md = render_markdown(&report_blocks(&ws, Some(&a)));
        assert_eq!(
            html.matches("<h2>").count(),
            md.lines().filter(|l| l.starts_with("## ")).count()
        );
    }

    #[test]
    fn a_report_with_one_database_still_works() {
        let (mut ws, _) = sample();
        ws.clear(Side::Target);
        let md = report_markdown(&ws, None);
        assert!(md.contains("## All findings"));
        assert!(md.contains("Load both databases"));
        assert!(!md.contains("## Mapping"));
        let s = summary(&ws, None);
        assert_eq!(s.tables, TableCounts::default());
        assert_eq!(s.problems.len(), 1);
    }

    #[test]
    fn markdown_tables_escape_pipes() {
        let md = render_markdown(&[Block::Table {
            head: vec!["a|b".into()],
            rows: vec![vec!["x|y\nz".into()]],
        }]);
        assert!(md.contains("| a\\|b |") && md.contains("| x\\|y z |"));
    }
}
