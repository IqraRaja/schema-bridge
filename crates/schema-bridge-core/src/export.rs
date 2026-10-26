//! Exports for a meeting or a ticket (FR-18): the findings and the mapping
//! as CSV, the findings as a table to paste into a spreadsheet, and a short
//! report (see `report`). Everything is plain text produced from the
//! workspace; saving it is up to the application.

use crate::checks::Finding;
use crate::matching::{ColumnStatus, MatchKind};
use crate::model::table_name;
use crate::workspace::{Analysis, Side, Workspace};

/// Who a finding is about: one of the databases, or fitting them together.
pub fn origin_label(ws: &Workspace, side: Option<Side>) -> String {
    match side {
        Some(s) => ws.slot(s).name.clone(),
        None => "Integration".into(),
    }
}

/// Every finding: inside the source, inside the target and in fitting them
/// together (`None`), most serious first.
pub fn all_findings(ws: &Workspace, analysis: Option<&Analysis>) -> Vec<(Option<Side>, Finding)> {
    let mut all: Vec<(Option<Side>, Finding)> = ws
        .findings()
        .into_iter()
        .map(|(s, f)| (Some(s), f))
        .collect();
    if let Some(a) = analysis {
        all.extend(a.integration.iter().cloned().map(|f| (None, f)));
    }
    // Stable: within a severity, source, target, then integration.
    all.sort_by_key(|(side, f)| (f.severity, side.map_or(2, |s| s as u8)));
    all
}

/// One CSV field. Fields that a spreadsheet would read as a formula get a
/// leading `'`, so opening the file never runs anything.
fn csv_field(value: &str) -> String {
    let value = if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_string()
    };
    if value.contains([',', '"', '\n', '\r']) || value.trim() != value {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

fn csv(rows: &[Vec<String>]) -> String {
    rows.iter()
        .map(|r| r.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(",") + "\r\n")
        .collect()
}

const FINDING_HEADER: [&str; 8] = [
    "Severity",
    "Database",
    "Area",
    "Rule",
    "Object",
    "Finding",
    "What it means",
    "What to do",
];

fn finding_rows(ws: &Workspace, findings: &[(Option<Side>, Finding)]) -> Vec<Vec<String>> {
    let mut rows = vec![FINDING_HEADER.map(String::from).to_vec()];
    rows.extend(findings.iter().map(|(side, f)| {
        vec![
            f.severity.label().to_string(),
            origin_label(ws, *side),
            f.area.clone(),
            f.rule.to_string(),
            f.object.clone(),
            f.title.clone(),
            f.explanation.clone(),
            f.fix.clone(),
        ]
    }));
    rows
}

/// The findings as CSV (comma-separated, one finding per row).
pub fn findings_csv(ws: &Workspace, findings: &[(Option<Side>, Finding)]) -> String {
    csv(&finding_rows(ws, findings))
}

/// The findings as tab-separated text, to paste into a spreadsheet or a
/// ticket (the prototype's "Copy as table").
pub fn findings_table(ws: &Workspace, findings: &[(Option<Side>, Finding)]) -> String {
    finding_rows(ws, findings)
        .iter()
        .map(|r| {
            r.iter()
                .map(|f| f.replace(['\t', '\n', '\r'], " "))
                .collect::<Vec<_>>()
                .join("\t")
                + "\n"
        })
        .collect()
}

fn type_of(label: &str, data_type: &str) -> String {
    if label.is_empty() { data_type } else { label }.to_string()
}

/// The mapping matrix as CSV: every source column and where it goes, plus
/// the required target columns that nothing fills.
pub fn mapping_csv(ws: &Workspace, analysis: &Analysis) -> String {
    let (Some(source), Some(target)) = (&ws.source.schema, &ws.target.schema) else {
        return String::new();
    };
    let mut rows = vec![
        [
            "Source table",
            "Source column",
            "Source type",
            "Target table",
            "Target column",
            "Target type",
            "Result",
            "Type fit",
            "Type explanation",
            "Matched by",
            "Confidence",
            "Changed by you",
            "Notes",
        ]
        .map(String::from)
        .to_vec(),
    ];
    for s in &source.tables {
        let m = analysis.matching.table(&s.key);
        match analysis.matching.mapping(&s.key) {
            Some(p) => {
                let t = target.table(&p.target).map_or("", |t| t.name.as_str());
                for r in &p.rows {
                    let notes: Vec<&str> = r.notes.iter().map(|n| n.text.as_str()).collect();
                    rows.push(vec![
                        s.name.clone(),
                        r.source.name.clone(),
                        type_of(&r.source.type_label, &r.source.data_type),
                        if r.target.is_some() {
                            t.to_string()
                        } else {
                            String::new()
                        },
                        r.target
                            .as_ref()
                            .map(|c| c.name.clone())
                            .unwrap_or_default(),
                        r.target
                            .as_ref()
                            .map(|c| type_of(&c.type_label, &c.data_type))
                            .unwrap_or_default(),
                        r.status.label().to_string(),
                        r.compatibility
                            .as_ref()
                            .map(|c| c.fit.label().to_string())
                            .unwrap_or_default(),
                        r.compatibility
                            .as_ref()
                            .map(|c| c.note.clone())
                            .unwrap_or_default(),
                        r.how.map(|h| h.label().to_string()).unwrap_or_default(),
                        r.evidence
                            .filter(|_| !r.manual)
                            .map(|e| format!("{:.0}%", e.confidence() * 100.0))
                            .unwrap_or_default(),
                        if r.manual { "yes" } else { "" }.to_string(),
                        notes.join(" "),
                    ]);
                }
                for r in &p.required {
                    let note = match &r.link_to {
                        Some(k) => format!(
                            "Required link to {}; nothing in the source fills it.",
                            table_name(k)
                        ),
                        None => "Required; nothing in the source fills it.".to_string(),
                    };
                    rows.push(vec![
                        s.name.clone(),
                        String::new(),
                        String::new(),
                        t.to_string(),
                        r.column.name.clone(),
                        type_of(&r.column.type_label, &r.column.data_type),
                        "needs a decision".into(),
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new(),
                        note,
                    ]);
                }
            }
            None => {
                let stays = m.is_some_and(|m| m.kind == MatchKind::Stays);
                let status = if stays {
                    ColumnStatus::NeverCopy
                } else {
                    ColumnStatus::NotCopied
                };
                let note = if stays {
                    "The table stays in the source."
                } else {
                    "The table has no place in the target yet."
                };
                for c in &s.columns {
                    rows.push(vec![
                        s.name.clone(),
                        c.name.clone(),
                        type_of(&c.type_label, &c.data_type),
                        String::new(),
                        String::new(),
                        String::new(),
                        status.label().to_string(),
                        String::new(),
                        String::new(),
                        m.map(|m| m.decided_by.label().to_string())
                            .unwrap_or_default(),
                        String::new(),
                        if m.is_some_and(|m| m.decided_by == crate::matching::DecidedBy::User) {
                            "yes"
                        } else {
                            ""
                        }
                        .to_string(),
                        note.to_string(),
                    ]);
                }
            }
        }
    }
    csv(&rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (Workspace, Analysis) {
        let mut ws = Workspace::new();
        ws.load_samples();
        let a = ws.analysis().unwrap();
        (ws, a)
    }

    #[test]
    fn csv_fields_are_quoted_and_formulas_are_defused() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a, b"), "\"a, b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("two\nlines"), "\"two\nlines\"");
        assert_eq!(csv_field("=SUM(A1)"), "'=SUM(A1)");
        assert_eq!(csv_field("-1"), "'-1");
    }

    #[test]
    fn all_findings_are_exported_once_most_serious_first() {
        let (ws, a) = sample();
        let all = all_findings(&ws, Some(&a));
        assert_eq!(all.len(), 16 + 11);
        assert!(all.windows(2).all(|w| w[0].1.severity <= w[1].1.severity));
        let text = findings_csv(&ws, &all);
        assert_eq!(text.lines().count(), 1 + all.len());
        assert!(text.starts_with(
            "Severity,Database,Area,Rule,Object,Finding,What it means,What to do\r\n"
        ));
        assert!(text.contains(",Integration,"));

        let table = findings_table(&ws, &all);
        assert_eq!(table.lines().count(), 1 + all.len());
        assert!(table.lines().all(|l| l.split('\t').count() == 8));
    }

    #[test]
    fn the_mapping_lists_every_source_column_and_unfilled_required_columns() {
        let (ws, a) = sample();
        let text = mapping_csv(&ws, &a);
        let columns = ws.source.schema.as_ref().unwrap().stats().columns;
        let required: usize = a.matching.mappings.iter().map(|p| p.required.len()).sum();
        assert_eq!(text.lines().count(), 1 + columns + required);
        let serial = text
            .lines()
            .find(|l| l.starts_with("machine,serial_no,"))
            .unwrap();
        assert!(
            serial.contains(",asset,code,") && serial.contains(",check,Same,Same type,word list,")
        );
        // Every row has as many fields as the header (no field here holds a comma).
        assert_eq!(text.lines().next().unwrap().split(',').count(), 13);
        let id = text.lines().find(|l| l.starts_with("machine,id,")).unwrap();
        assert!(id.contains(",Incompatible,") && id.contains(",automatic,75%,,"));
        assert!(text.contains("booking,id,"));
        assert!(text.contains("needs a decision"));
    }
}
