//! Draft SQL for a test load: a port of the prototype's `castExpr`, `sqlFor`
//! and the script of its integration plan.
//!
//! The SQL is a recommendation for people to review. Schema Bridge never
//! runs it and never connects to a database: every open decision is marked
//! `TODO`, the script works on a staging copy and ends in a commented-out
//! `COMMIT`. It is written in PostgreSQL syntax (FR-16).

use crate::checks::quote;
use crate::matching::{ColumnRow, TableMapping};
use crate::model::{Schema, table_name};
use crate::plan::LoadStep;
use crate::systems::{foreign_data_wrapper, system_name};
use crate::types::{Category, Fit, type_info};

/// A value in an SQL string literal: `'` becomes `''`.
fn literal(text: &str) -> String {
    text.replace('\'', "''")
}

/// The expression that reads one source column for its target column, and
/// a comment for the reader (empty when nothing needs saying).
fn cast(row: &ColumnRow, mapping_target: &crate::model::Table) -> (String, String) {
    let (Some(tc), Some(cp)) = (&row.target, &row.compatibility) else {
        return ("NULL".into(), String::new());
    };
    let c = format!("s.{}", quote(&row.source.name));
    if matches!(cp.fit, Fit::Same | Fit::Safe) {
        return (c, String::new());
    }
    if cp.lookup {
        return match mapping_target.link_of(&tc.name) {
            Some(f) => (
                format!(
                    "(SELECT x.{} FROM {} x WHERE x.name = {c})",
                    quote(f.referenced_columns.first().map_or("uuid", String::as_str)),
                    quote(table_name(&f.references))
                ),
                "TODO: lookup by name — check the match column".into(),
            ),
            None => (
                format!("NULL /* TODO: look up the ID for {c} */"),
                "lookup needed".into(),
            ),
        };
    }
    let (a, b) = (type_info(&row.source.data_type), type_info(&tc.data_type));
    if a.category == Category::Timestamp
        && b.category == Category::Timestamp
        && a.time_zone != Some(true)
    {
        return (
            format!("({c} AT TIME ZONE '<source time zone>')"),
            "TODO: confirm the source time zone".into(),
        );
    }
    if cp.fit == Fit::Incompatible {
        return (
            format!("NULL /* TODO: {}: {} */", row.source.name, cp.note),
            String::new(),
        );
    }
    let ty = if tc.type_label.is_empty() {
        &tc.data_type
    } else {
        &tc.type_label
    };
    (format!("{c}::{ty}"), cp.note.clone())
}

/// The `INSERT … SELECT` that loads one source table into its target table
/// from the staging copy.
pub fn table_sql(
    mapping: &TableMapping,
    source: &Schema,
    target: &Schema,
    source_name: &str,
) -> String {
    let (Some(s), Some(t)) = (source.table(&mapping.source), target.table(&mapping.target)) else {
        return String::new();
    };
    let mut items: Vec<(String, String, String)> = Vec::new();
    for r in mapping.rows.iter().filter(|r| r.target.is_some()) {
        let (e, n) = cast(r, t);
        items.push((r.target.as_ref().unwrap().name.clone(), e, n));
    }
    for r in &mapping.required {
        let link = r
            .link_to
            .as_deref()
            .map(|k| format!(" link to {}", table_name(k)))
            .unwrap_or_default();
        items.push((
            r.column.name.clone(),
            "NULL".into(),
            format!("TODO: required{link}, no source column"),
        ));
    }
    if let (Some(trace), [id]) = (&mapping.trace, s.primary_key.as_slice()) {
        items.push((
            trace.clone(),
            format!(
                "jsonb_build_object('source_system', '{}', 'source_table', '{}', 'source_id', s.{})",
                literal(source_name),
                literal(&s.name),
                quote(id)
            ),
            "keeps the original ID".into(),
        ));
    }
    let skipped: Vec<&str> = mapping
        .rows
        .iter()
        .filter(|r| r.target.is_none() && r.status != crate::matching::ColumnStatus::NeverCopy)
        .map(|r| r.source.name.as_str())
        .collect();

    let mut out = format!("-- {} → {}\n", s.name, t.name);
    if !skipped.is_empty() {
        out.push_str(&format!("-- not copied: {}\n", skipped.join(", ")));
    }
    let last = items.len().saturating_sub(1);
    out.push_str(&format!(
        "INSERT INTO {}.{} (\n",
        quote(&t.schema),
        quote(&t.name)
    ));
    for (i, (col, _, _)) in items.iter().enumerate() {
        let comma = if i < last { "," } else { "" };
        out.push_str(&format!("  {}{comma}\n", quote(col)));
    }
    out.push_str(")\nSELECT\n");
    let lines: Vec<String> = items
        .iter()
        .enumerate()
        .map(|(i, (_, e, n))| {
            let comma = if i < last { "," } else { "" };
            let note = if n.is_empty() {
                String::new()
            } else {
                format!("  -- {n}")
            };
            format!("  {e}{comma}{note}")
        })
        .collect();
    out.push_str(&lines.join("\n"));
    out.push_str(&format!("\nFROM staging.{} s;\n", quote(&s.name)));
    out
}

/// The whole draft script: make the source readable as a staging schema,
/// then load the tables in the load order inside one transaction that is
/// never committed by the script itself.
pub fn draft_sql(
    source: &Schema,
    target: &Schema,
    source_name: &str,
    target_name: &str,
    mappings: &[TableMapping],
    order: &[LoadStep],
) -> String {
    let mut out = String::new();
    if target.dbms != "postgresql" {
        out.push_str(&format!(
            "-- NOTE: {target_name} is {}. This draft is written in PostgreSQL syntax;\n-- use it as a column-by-column guide and adapt the statements.\n\n",
            system_name(&target.dbms)
        ));
    }
    // The source schema most tables come from (first one on a tie).
    let mut schemas: Vec<(&str, usize)> = Vec::new();
    for p in mappings {
        let schema = source.table(&p.source).map_or("", |s| s.schema.as_str());
        match schemas.iter_mut().find(|(s, _)| *s == schema) {
            Some((_, n)) => *n += 1,
            None => schemas.push((schema, 1)),
        }
    }
    let most = schemas.iter().map(|(_, n)| *n).max().unwrap_or(0);
    let source_schema = schemas
        .iter()
        .find(|(_, n)| *n == most)
        .map_or("public", |(s, _)| *s);
    let mut tables: Vec<String> = Vec::new();
    for p in mappings {
        let name = quote(table_name(&p.source));
        if !tables.contains(&name) {
            tables.push(name);
        }
    }

    match foreign_data_wrapper(&source.dbms, source_name) {
        Some((fdw, server, user)) => {
            out.push_str(&format!(
                "-- 1. Make the {source_name} tables ({}) readable inside {target_name} (run once, as admin).\n",
                system_name(&source.dbms)
            ));
            out.push_str(&format!(
                "--    Needs the {fdw} extension. Alternative: export each table to CSV and load it with COPY into staging.\n"
            ));
            out.push_str(&format!("CREATE EXTENSION IF NOT EXISTS {fdw};\n"));
            out.push_str(&format!(
                "CREATE SERVER source_db FOREIGN DATA WRAPPER {fdw}\n  OPTIONS ({server});\n"
            ));
            if let Some(user) = user {
                out.push_str(&format!(
                    "CREATE USER MAPPING FOR CURRENT_USER SERVER source_db\n  OPTIONS ({user});\n"
                ));
            }
        }
        // A system without a known wrapper: staging through CSV only.
        None => out.push_str(&format!(
            "-- 1. Make the {source_name} tables ({}) readable inside {target_name} as the schema \"staging\".\n--    No foreign data wrapper is known for this system: export each table to CSV and load it with COPY into staging.\n",
            system_name(&source.dbms)
        )),
    }
    out.push_str("CREATE SCHEMA IF NOT EXISTS staging;\n");
    out.push_str(&format!(
        "IMPORT FOREIGN SCHEMA {}\n  LIMIT TO ({})\n  FROM SERVER source_db INTO staging;\n\n",
        quote(source_schema),
        tables.join(", ")
    ));
    out.push_str(
        "-- 2. Load in this order, on the TEST database, inside one transaction\nBEGIN;\n\n",
    );
    let statements: Vec<String> = order
        .iter()
        .flat_map(|step| mappings.iter().filter(move |p| p.target == step.table))
        .map(|p| table_sql(p, source, target, source_name))
        .collect();
    out.push_str(&statements.join("\n"));
    out.push_str("\n-- 3. Check the results (counts, links), then:\n-- COMMIT;   or   ROLLBACK;\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_double_their_quotes() {
        assert_eq!(literal("O'Brien's"), "O''Brien''s");
    }

    #[test]
    fn a_target_that_is_not_postgresql_gets_a_note_and_the_script_never_commits() {
        let target = Schema {
            dbms: "mysql".into(),
            ..Default::default()
        };
        let sql = draft_sql(&Schema::default(), &target, "S", "T", &[], &[]);
        assert!(sql.starts_with("-- NOTE: T is MySQL / MariaDB."));
        assert!(sql.contains("BEGIN;"));
        assert!(sql.ends_with("-- COMMIT;   or   ROLLBACK;\n"));
        assert!(!sql.lines().any(|l| l.trim() == "COMMIT;"));
    }

    #[test]
    fn a_source_without_a_known_wrapper_is_staged_through_csv() {
        let source = Schema {
            dbms: "db2".into(),
            ..Default::default()
        };
        let sql = draft_sql(
            &source,
            &Schema {
                dbms: "postgresql".into(),
                ..Default::default()
            },
            "S",
            "T",
            &[],
            &[],
        );
        assert!(sql.contains("No foreign data wrapper is known"));
        assert!(!sql.contains("CREATE SERVER"));
        assert!(sql.contains("CREATE SCHEMA IF NOT EXISTS staging;"));
    }
}
