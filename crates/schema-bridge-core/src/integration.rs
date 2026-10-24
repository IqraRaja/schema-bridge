//! Problems in fitting the two databases together: a port of the
//! prototype's `integrationChecks`. They are read from the matching (see
//! `matching`) and explained in plain language, like the findings inside
//! each database.

use crate::checks::{Finding, Severity};
use crate::influx::{describe_retention, retention_of};
use crate::matching::{MatchKind, Matching, NoteKind};
use crate::model::{ColumnRole, Schema, Table, TableKind, table_name};
use crate::workspace::grouped;

/// Rule ids of integration findings (specification §7, "Integration checks").
pub const INTEGRATION_RULES: &[&str] = &[
    "MAP-REQUIRED-LINK",
    "MAP-REQUIRED-VALUE",
    "MAP-RELATIONSHIP",
    "MAP-DUPLICATES",
    "MAP-TYPE-FIT",
    "MAP-EMPTY-VALUES",
    "MAP-NO-SOURCE-ID",
    "MAP-MANY-TO-ONE",
    "MAP-NO-TARGET",
];

/// Rule ids of the integration checks that involve a time-series
/// measurement (docs/time-series.md).
pub const TIME_SERIES_INTEGRATION_RULES: &[&str] = &[
    "MAP-TS-SERIES-KEY",
    "MAP-TS-RETENTION",
    "MAP-TS-ROLE-CHANGE",
];

/// Checks how well the source fits into the target, given the matching.
/// The findings are sorted by severity, otherwise in the order the checks
/// report them (pair by pair, then the whole picture).
pub fn integration_findings(
    source: &Schema,
    target: &Schema,
    source_name: &str,
    target_name: &str,
    matching: &Matching,
) -> Vec<Finding> {
    let (sn, tn) = (source_name, target_name);
    let mut out = Vec::new();
    let mut add = |rule: &'static str,
                   severity: Severity,
                   area: &str,
                   object: String,
                   title: String,
                   explanation: String,
                   fix: String| {
        out.push(Finding {
            rule,
            severity,
            area: area.to_string(),
            object,
            title,
            explanation,
            fix,
        })
    };

    for p in &matching.mappings {
        let (Some(s), Some(t)) = (source.table(&p.source), target.table(&p.target)) else {
            continue;
        };
        let pair = format!("{} → {}", s.name, t.name);
        for r in &p.required {
            let c = &r.column.name;
            match &r.link_to {
                Some(link) => add(
                    "MAP-REQUIRED-LINK",
                    Severity::Blocking,
                    "Mapping",
                    format!("{}.{c}", t.name),
                    format!("Required link has no source: {c}"),
                    format!(
                        "Every row in {tn} {} needs a {c} (a link to {}). {sn} {} has nothing that matches it, so the load will fail until someone decides the value.",
                        t.name,
                        table_name(link),
                        s.name
                    ),
                    format!(
                        "Decide how to fill it: for example a small lookup list \"{} row → {} row\", or one default value. Then add it to the import.",
                        s.name,
                        table_name(link)
                    ),
                ),
                None => add(
                    "MAP-REQUIRED-VALUE",
                    Severity::High,
                    "Mapping",
                    format!("{}.{c}", t.name),
                    format!("Required value has no source: {c}"),
                    format!(
                        "{tn} {}.{c} must be filled, but no {} column matches it.",
                        t.name, s.name
                    ),
                    "Map a source column to it, or decide a fixed value.".into(),
                ),
            }
        }
        for r in &p.rows {
            let Some(tc) = &r.target else { continue };
            let sc = &r.source;
            for n in &r.notes {
                match n.kind {
                    NoteKind::Relationship => add(
                        "MAP-RELATIONSHIP",
                        Severity::High,
                        "Relationship",
                        format!("{pair}: {} → {}", sc.name, tc.name),
                        "Relationship does not line up".into(),
                        format!(
                            "{} The two databases connect this data at different levels.",
                            n.text
                        ),
                        "Decide which level is right. You may need an extra table in the target, or a lookup from source rows to target rows.".into(),
                    ),
                    NoteKind::RoleChange => add(
                        "MAP-TS-ROLE-CHANGE",
                        Severity::Medium,
                        "Time series",
                        format!("{pair}: {} → {}", sc.name, tc.name),
                        "Tag and field roles differ".into(),
                        n.text.clone(),
                        "Decide which role the value should have in the target; a tag identifies a series, a field holds a measured value.".into(),
                    ),
                    NoteKind::Unique => add(
                        "MAP-DUPLICATES",
                        Severity::High,
                        "Duplicates",
                        format!("{pair}: {} → {}", sc.name, tc.name),
                        "Duplicates would be rejected".into(),
                        format!(
                            "{tn} requires {} to be unique, but {sn} {} has no unique rule. If two rows share a value, the import fails.",
                            tc.name, sc.name
                        ),
                        format!("Run a duplicate check on {}.{} before loading.", s.name, sc.name),
                    ),
                    NoteKind::Type if n.severity == Severity::High => {
                        let ty = |label: &str, data_type: &str| {
                            if label.is_empty() {
                                data_type.to_string()
                            } else {
                                label.to_string()
                            }
                        };
                        add(
                            "MAP-TYPE-FIT",
                            Severity::Medium,
                            "Types",
                            format!(
                                "{pair}: {} ({}) → {} ({})",
                                sc.name,
                                ty(&sc.type_label, &sc.data_type),
                                tc.name,
                                ty(&tc.type_label, &tc.data_type)
                            ),
                            "Values may not fit".into(),
                            n.text.clone(),
                            "Check the longest or largest values before loading.".into(),
                        )
                    }
                    _ => {}
                }
            }
        }
        let empty: Vec<&str> = p
            .rows
            .iter()
            .filter(|r| r.notes.iter().any(|n| n.kind == NoteKind::EmptyValues))
            .map(|r| r.source.name.as_str())
            .collect();
        if !empty.is_empty() {
            add(
                "MAP-EMPTY-VALUES",
                Severity::Medium,
                "Mapping",
                format!("{pair}: {}", empty.join(", ")),
                "Empty values would be rejected".into(),
                format!(
                    "These {sn} columns may be empty, but the matching {tn} columns require a value."
                ),
                "Count empty values in the source; decide a default or filter those rows out."
                    .into(),
            );
        }
        let keeps_id = p.rows.iter().any(|r| {
            r.target.is_some() && s.primary_key.first().is_some_and(|k| *k == r.source.name)
        });
        // A measurement has no source ID: its points are identified by tags
        // and time, which MAP-TS-SERIES-KEY covers.
        if p.trace.is_none() && !keeps_id && s.kind != TableKind::Measurement {
            add(
                "MAP-NO-SOURCE-ID",
                Severity::Medium,
                "Traceability",
                pair.clone(),
                "No place to keep the source ID".into(),
                format!(
                    "{} has no field for the original {sn} ID. Without it you cannot re-run the import safely or trace a record back.",
                    t.name
                ),
                "Create a mapping table (source ID → target ID) or store the source ID in a JSON column.".into(),
            );
        }
    }

    // Several source tables into one target table.
    let mut into: Vec<(&str, Vec<&str>)> = Vec::new();
    for p in &matching.mappings {
        let name = table_name(&p.source);
        match into.iter_mut().find(|(k, _)| *k == p.target) {
            Some((_, names)) => names.push(name),
            None => into.push((&p.target, vec![name])),
        }
    }
    for (key, names) in into.iter().filter(|(_, n)| n.len() > 1) {
        add(
            "MAP-MANY-TO-ONE",
            Severity::Info,
            "Mapping",
            format!("{} → {}", names.join(", "), table_name(key)),
            "Several source tables go into one target table".into(),
            format!(
                "Rows from {} would be mixed in {}.",
                names.join(" and "),
                table_name(key)
            ),
            "Make sure you can still tell them apart, for example with a type value.".into(),
        );
    }

    for s in &source.tables {
        let Some(m) = matching.table(&s.key) else {
            continue;
        };
        if m.kind != MatchKind::NoTarget {
            continue;
        }
        let rows = s
            .rows
            .filter(|r| *r > 0)
            .map(|r| format!(", about {} rows", grouped(r)))
            .unwrap_or_default();
        let closest = m
            .candidate
            .as_ref()
            .map(|c| format!(" Closest candidate: {}.", table_name(&c.target)))
            .unwrap_or_default();
        add(
            "MAP-NO-TARGET",
            Severity::Medium,
            "No target",
            s.name.clone(),
            format!("No place in {tn} for {}", s.name),
            format!(
                "{} ({} columns{rows}) has no matching table in {tn}.{closest}",
                s.name,
                s.columns.len()
            ),
            format!(
                "Decide: extend {tn} with a new table, store the data in a JSON field, or keep it in {sn} only."
            ),
        );
    }

    out.extend(time_series_findings(
        source,
        target,
        source_name,
        target_name,
        matching,
    ));
    out.sort_by_key(|f| f.severity);
    out
}

/// Whether a table has a unique rule (primary key, unique rule or unique
/// index that is not partial) on exactly these columns.
fn unique_on(table: &Table, columns: &[String]) -> bool {
    let same =
        |list: &[String]| list.len() == columns.len() && columns.iter().all(|c| list.contains(c));
    (!table.primary_key.is_empty() && same(&table.primary_key))
        || table.uniques.iter().any(|u| same(&u.columns))
        || table
            .indexes
            .iter()
            .any(|i| i.unique && !i.partial && same(&i.columns))
}

/// Integration checks for mappings that involve a time-series measurement.
/// Mappings between two relational tables are never looked at here.
fn time_series_findings(
    source: &Schema,
    target: &Schema,
    sn: &str,
    tn: &str,
    matching: &Matching,
) -> Vec<Finding> {
    let mut out = Vec::new();
    for p in &matching.mappings {
        let (Some(s), Some(t)) = (source.table(&p.source), target.table(&p.target)) else {
            continue;
        };
        let (s_ts, t_ts) = (
            s.kind == TableKind::Measurement,
            t.kind == TableKind::Measurement,
        );
        if !s_ts && !t_ts {
            continue;
        }
        let pair = format!("{} → {}", s.name, t.name);
        let target_of = |source_column: &str| {
            p.rows
                .iter()
                .find(|r| r.source.name == source_column)
                .and_then(|r| r.target.as_ref())
                .map(|c| c.name.clone())
        };
        let finding =
            |rule: &'static str, severity, title: &str, explanation: String, fix: &str| Finding {
                rule,
                severity,
                area: "Time series".into(),
                object: pair.clone(),
                title: title.into(),
                explanation,
                fix: fix.into(),
            };

        // Point identity: tags + time.
        if s_ts && !t_ts {
            let identity: Vec<&str> = s
                .columns
                .iter()
                .filter(|c| matches!(c.role, ColumnRole::Tag | ColumnRole::Timestamp))
                .map(|c| c.name.as_str())
                .collect();
            let mapped: Option<Vec<String>> = identity.iter().map(|c| target_of(c)).collect();
            if let Some(columns) = mapped.filter(|c| !c.is_empty()) {
                if !unique_on(t, &columns) {
                    out.push(finding(
                        "MAP-TS-SERIES-KEY",
                        Severity::Medium,
                        "No unique rule on the point identity",
                        format!(
                            "InfluxDB identifies a point by its tags and time: a second point with the same {} replaces the first. {tn} {} has no unique rule on ({}), so loading overlapping exports twice would store duplicate rows.",
                            identity.join(", "),
                            t.name,
                            columns.join(", ")
                        ),
                        "Review whether a unique rule on these columns fits your data. Schema Bridge does not add it automatically.",
                    ));
                }
            }
        }
        if t_ts && !s_ts {
            let identity: Vec<String> = p
                .rows
                .iter()
                .filter(|r| {
                    r.target
                        .as_ref()
                        .is_some_and(|c| matches!(c.role, ColumnRole::Tag | ColumnRole::Timestamp))
                })
                .map(|r| r.source.name.clone())
                .collect();
            if !identity.is_empty() && !unique_on(s, &identity) {
                out.push(finding(
                    "MAP-TS-SERIES-KEY",
                    Severity::Medium,
                    "Rows may overwrite each other",
                    format!(
                        "InfluxDB keeps one point per series and time. {sn} {} has no unique rule on ({}), the columns that become tags and time, so rows that share these values would silently overwrite each other.",
                        s.name,
                        identity.join(", ")
                    ),
                    "Check the source for duplicates on these columns, or map another column to a tag that tells them apart.",
                ));
            }
        }

        // Retention.
        let source_retention = if s_ts { retention_of(source, s) } else { None };
        let target_retention = if t_ts { retention_of(target, t) } else { None };
        match (source_retention, target_retention) {
            (Some(Some(src)), _) if !t_ts => out.push(finding(
                "MAP-TS-RETENTION",
                Severity::Medium,
                "Source keeps a limited history",
                format!(
                    "{sn} keeps {} of data for {}, so only that period exists to migrate. {tn} has no retention period: {} keeps rows until they are deleted. If the target needs a longer history, older data is not available; if it should keep only {}, plan regular deletion.",
                    describe_retention(Some(src)),
                    s.name,
                    t.name,
                    describe_retention(Some(src))
                ),
                "Decide how much history the target needs and how old rows are removed.",
            )),
            (source, Some(Some(tgt))) if source.is_none_or(|s| s.is_none_or(|s| s > tgt)) => out.push(finding(
                "MAP-TS-RETENTION",
                Severity::High,
                "Target keeps data for a shorter time",
                format!(
                    "{tn} keeps {} for {}, while the source keeps {}. Points older than {} are dropped when they are written.",
                    describe_retention(Some(tgt)),
                    t.name,
                    match source {
                        Some(r) => describe_retention(r),
                        None => "data without a known limit".to_string(),
                    },
                    describe_retention(Some(tgt))
                ),
                "Lengthen the target's retention, or accept that older points are lost.",
            )),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matching::{Overrides, WordList, match_schemas};
    use crate::model::{Column, ForeignKey, Table};

    fn column(name: &str, data_type: &str, nullable: bool) -> Column {
        Column {
            name: name.into(),
            type_label: data_type.into(),
            data_type: data_type.into(),
            nullable,
            ..Default::default()
        }
    }

    fn table(schema: &str, name: &str, columns: Vec<Column>) -> Table {
        Table {
            key: format!("{schema}.{name}"),
            schema: schema.into(),
            name: name.into(),
            columns,
            primary_key: vec!["id".into()],
            ..Default::default()
        }
    }

    fn schema(tables: Vec<Table>) -> Schema {
        Schema {
            dbms: "postgresql".into(),
            tables,
            ..Default::default()
        }
    }

    fn rules(findings: &[Finding]) -> Vec<&'static str> {
        findings.iter().map(|f| f.rule).collect()
    }

    #[test]
    fn a_relationship_that_does_not_line_up_is_reported() {
        // Source: task → project. Target: task → team, while project goes into project.
        let mut s_task = table(
            "s",
            "task",
            vec![
                column("id", "integer", false),
                column("project_id", "integer", true),
            ],
        );
        s_task.foreign_keys.push(ForeignKey {
            columns: vec!["project_id".into()],
            references: "s.project".into(),
            ..Default::default()
        });
        let mut t_task = table(
            "t",
            "task",
            vec![
                column("id", "integer", false),
                column("project_id", "integer", true),
            ],
        );
        t_task.foreign_keys.push(ForeignKey {
            columns: vec!["project_id".into()],
            references: "t.team".into(),
            ..Default::default()
        });
        let source = schema(vec![
            table("s", "project", vec![column("id", "integer", false)]),
            s_task,
        ]);
        let target = schema(vec![
            table("t", "project", vec![column("id", "integer", false)]),
            table("t", "team", vec![column("id", "integer", false)]),
            t_task,
        ]);
        let m = match_schemas(
            &source,
            &target,
            &WordList::default(),
            &Overrides::default(),
        );
        let f = integration_findings(&source, &target, "S", "T", &m);
        assert_eq!(rules(&f), ["MAP-RELATIONSHIP"]);
        assert_eq!(f[0].severity, Severity::High);
        assert!(f[0].explanation.contains("which goes into project"));
    }

    #[test]
    fn no_source_id_several_sources_and_tables_without_a_place() {
        let source = schema(vec![
            table("s", "note", vec![column("text", "text", true)]),
            table("s", "notes", vec![column("text", "text", true)]),
            table("s", "zzz", vec![column("qqq", "text", true)]),
        ]);
        let mut note = table("t", "note", vec![column("text", "text", true)]);
        note.primary_key.clear();
        let target = schema(vec![note]);
        let m = match_schemas(
            &source,
            &target,
            &WordList::default(),
            &Overrides::default(),
        );
        let f = integration_findings(&source, &target, "S", "T", &m);
        assert_eq!(
            rules(&f),
            [
                "MAP-NO-SOURCE-ID",
                "MAP-NO-SOURCE-ID",
                "MAP-NO-TARGET",
                "MAP-MANY-TO-ONE"
            ]
        );
        assert_eq!(f[3].object, "note, notes → note");
        assert_eq!(f[2].title, "No place in T for zzz");
    }

    #[test]
    fn every_rule_is_produced_in_some_situation_and_no_other_id_appears() {
        use crate::matching::TableChoice;
        use crate::workspace::Workspace;
        use std::collections::BTreeSet;

        let mut seen: BTreeSet<&'static str> = BTreeSet::new();
        // The sample pair with choices that send two tables into one target,
        // leave a table without a place and map text into a link.
        let mut ws = Workspace::new();
        ws.load_samples();
        ws.set_table_choice(
            "main.booking",
            Some(TableChoice::Target("public.session".into())),
        );
        ws.set_table_choice("main.operator", Some(TableChoice::NoTarget));
        ws.set_column_choice("main.machine", "room", Some(Some("location_id".into())));
        seen.extend(ws.analysis().unwrap().integration.iter().map(|f| f.rule));
        // The two small situations of the tests above.
        for (source, target) in [relationship_pair(), id_and_many_pair()] {
            let m = match_schemas(
                &source,
                &target,
                &WordList::default(),
                &Overrides::default(),
            );
            seen.extend(
                integration_findings(&source, &target, "S", "T", &m)
                    .iter()
                    .map(|f| f.rule),
            );
        }
        let known: BTreeSet<&'static str> = INTEGRATION_RULES.iter().copied().collect();
        assert_eq!(
            seen, known,
            "every rule fires somewhere, and only known ids appear"
        );
    }

    fn relationship_pair() -> (Schema, Schema) {
        let link = |from: &str, to: &str| ForeignKey {
            columns: vec![from.into()],
            references: to.into(),
            ..Default::default()
        };
        let task = |schema: &str, to: &str| {
            let mut t = table(
                schema,
                "task",
                vec![
                    column("id", "integer", false),
                    column("project_id", "integer", true),
                ],
            );
            t.foreign_keys.push(link("project_id", to));
            t
        };
        (
            schema(vec![
                table("s", "project", vec![column("id", "integer", false)]),
                task("s", "s.project"),
            ]),
            schema(vec![
                table("t", "project", vec![column("id", "integer", false)]),
                table("t", "team", vec![column("id", "integer", false)]),
                task("t", "t.team"),
            ]),
        )
    }

    fn id_and_many_pair() -> (Schema, Schema) {
        let mut note = table("t", "note", vec![column("text", "text", true)]);
        note.primary_key.clear();
        (
            schema(vec![
                table("s", "note", vec![column("text", "text", true)]),
                table("s", "notes", vec![column("text", "text", true)]),
            ]),
            schema(vec![note]),
        )
    }
}
