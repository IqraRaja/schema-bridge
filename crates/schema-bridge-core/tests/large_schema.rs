//! Schema Bridge must stay usable on large schemas: the whole analysis of
//! two databases with hundreds of tables has to finish quickly.

use std::time::Instant;

use schema_bridge_core::model::{Column, ForeignKey, Schema, Table};
use schema_bridge_core::workspace::Workspace;

/// A made-up schema with `tables` tables of 12 columns, each linking to the
/// previous table. `prefix` makes the two sides use different words.
fn schema(dbms: &str, schema_name: &str, prefix: &str, tables: usize) -> Schema {
    let words = [
        "name",
        "code",
        "created_at",
        "amount",
        "status",
        "note",
        "owner",
        "size",
        "label",
        "kind",
    ];
    let mut out = Schema {
        dbms: dbms.into(),
        ..Default::default()
    };
    for i in 0..tables {
        let name = format!("{prefix}_entity_{i}");
        let mut t = Table {
            key: format!("{schema_name}.{name}"),
            schema: schema_name.into(),
            name: name.clone(),
            primary_key: vec!["id".into()],
            rows: Some(1000),
            ..Default::default()
        };
        t.columns.push(Column {
            name: "id".into(),
            type_label: "integer".into(),
            data_type: "integer".into(),
            position: 1,
            ..Default::default()
        });
        for (j, w) in words.iter().enumerate() {
            t.columns.push(Column {
                name: format!("{w}_{}", (i + j) % 7),
                type_label: "text".into(),
                data_type: "text".into(),
                nullable: true,
                position: j as i64 + 2,
                ..Default::default()
            });
        }
        if i > 0 {
            t.columns.push(Column {
                name: "parent_id".into(),
                type_label: "integer".into(),
                data_type: "integer".into(),
                nullable: true,
                position: 13,
                ..Default::default()
            });
            t.foreign_keys.push(ForeignKey {
                columns: vec!["parent_id".into()],
                references: format!("{schema_name}.{prefix}_entity_{}", i - 1),
                referenced_columns: vec!["id".into()],
                ..Default::default()
            });
        }
        out.tables.push(t);
    }
    out.tables.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[test]
fn a_large_pair_is_analysed_quickly() {
    let n = 400;
    let mut ws = Workspace::new();
    ws.source.schema = Some(schema("mysql", "app", "src", n));
    ws.target.schema = Some(schema("postgresql", "public", "tgt", n));
    let start = Instant::now();
    let analysis = ws.analysis().expect("both loaded");
    let findings = ws.findings();
    let sql = ws.draft_sql(&analysis);
    let elapsed = start.elapsed();
    eprintln!(
        "{n} × {n} tables: {elapsed:?}, {} findings, {} mapped",
        findings.len(),
        analysis.matching.mappings.len()
    );
    assert_eq!(analysis.matching.tables.len(), n);
    assert!(!sql.is_empty());
    // Generous even for an unoptimised debug build on a slow machine; it
    // takes about a second there and a third of a second in release.
    assert!(elapsed.as_secs() < 30, "took {elapsed:?}");
}
