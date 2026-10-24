//! Parity tests: the Rust import must build the same model as the prototype.
//!
//! The inputs are the made-up demo exports in the repository's `demo/`
//! folder. The `tests/fixtures/*.expected.txt` files were produced by running
//! the prototype's own `buildModel` on those exports and printing the model in
//! the format of `summary` below (see docs/decisions.md).

use std::fs;
use std::path::PathBuf;

use schema_bridge_core::{ExportFiles, ExportKind, Schema, build_schema};

fn read(path: PathBuf) -> String {
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// A file from the repository's `demo/` folder.
fn demo(name: &str) -> String {
    read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../demo")
            .join(name),
    )
}

/// A file from this crate's `tests/fixtures/` folder.
fn fixture(name: &str) -> String {
    read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
}

fn load(names: &[&str]) -> Schema {
    let mut files = ExportFiles::default();
    for name in names {
        files.add(demo(name)).expect("demo file is a valid export");
    }
    build_schema(&files)
}

/// Prints the model in the same format as the prototype script that wrote
/// the expected files.
fn summary(s: &Schema) -> String {
    fn e(v: Option<&str>) -> String {
        v.map_or("null".to_string(), |v| {
            v.replace('\\', "\\\\")
                .replace('\n', "\\n")
                .replace('\r', "\\r")
        })
    }
    let b = |v: bool| u8::from(v);
    let mut out = vec![
        format!("dbms={}", s.dbms),
        format!(
            "has=cols:{},cons:{},idx:{},rows:{}",
            b(s.has.columns),
            b(s.has.constraints),
            b(s.has.indexes),
            b(s.has.row_counts)
        ),
        format!(
            "views={}",
            s.views.iter().cloned().collect::<Vec<_>>().join(",")
        ),
    ];
    for t in &s.tables {
        out.push(format!(
            "table {} schema={} name={} rows={} triggers={} comment={}",
            t.key,
            t.schema,
            t.name,
            t.rows.map_or("null".into(), |r| r.to_string()),
            e(Some(&t.triggers)),
            e(t.comment.as_deref())
        ));
        for c in &t.columns {
            out.push(format!(
                "  col {} {} label={} type={} nullable={} default={} comment={}",
                c.position,
                c.name,
                e(Some(&c.type_label)),
                e(Some(&c.data_type)),
                c.nullable,
                e(c.default.as_deref()),
                e(c.comment.as_deref())
            ));
        }
        out.push(format!(
            "  pk={} pk_name={}",
            t.primary_key.join(","),
            e(t.primary_key_name.as_deref())
        ));
        for u in &t.uniques {
            out.push(format!(
                "  uq {} ({})",
                u.name.as_deref().unwrap_or("null"),
                u.columns.join(",")
            ));
        }
        for f in &t.foreign_keys {
            out.push(format!(
                "  fk {} ({}) -> {}({}) not_valid={} on_delete={}",
                f.name.as_deref().unwrap_or("null"),
                f.columns.join(","),
                f.references,
                f.referenced_columns.join(","),
                f.not_valid,
                e(f.on_delete.as_deref())
            ));
        }
        for k in &t.checks {
            out.push(format!(
                "  check {} {}",
                k.name.as_deref().unwrap_or("null"),
                e(Some(&k.definition))
            ));
        }
        for i in &t.indexes {
            out.push(format!(
                "  idx {} unique={} partial={} ({})",
                i.name.as_deref().unwrap_or("null"),
                i.unique,
                i.partial,
                i.columns.join(",")
            ));
        }
    }
    out.join("\n") + "\n"
}

fn assert_same_as_prototype(schema: &Schema, expected_file: &str) {
    let expected = fixture(expected_file);
    let actual = summary(schema);
    if actual != expected {
        for (n, (a, x)) in actual.lines().zip(expected.lines()).enumerate() {
            assert_eq!(a, x, "first difference at line {}", n + 1);
        }
        assert_eq!(actual, expected, "different number of lines");
    }
}

#[test]
fn sqlite_one_file_export_matches_prototype() {
    let schema = load(&["workshop-sqlite.csv"]);
    assert_same_as_prototype(&schema, "workshop-sqlite.expected.txt");
}

#[test]
fn postgresql_one_file_export_matches_prototype() {
    let schema = load(&["catalogue-postgresql.csv"]);
    assert_same_as_prototype(&schema, "catalogue-postgresql.expected.txt");
}

#[test]
fn postgresql_four_file_export_matches_prototype() {
    let schema = load(&[
        "catalogue-postgresql-4files/columns.csv",
        "catalogue-postgresql-4files/constraints.csv",
        "catalogue-postgresql-4files/indexes.csv",
        "catalogue-postgresql-4files/rowcounts.csv",
    ]);
    assert_same_as_prototype(&schema, "catalogue-postgresql-4files.expected.txt");
}

#[test]
fn both_export_formats_give_the_same_model_apart_from_order() {
    let one = load(&["catalogue-postgresql.csv"]);
    let four = load(&[
        "catalogue-postgresql-4files/columns.csv",
        "catalogue-postgresql-4files/constraints.csv",
        "catalogue-postgresql-4files/indexes.csv",
        "catalogue-postgresql-4files/rowcounts.csv",
    ]);
    let sorted = |s: &Schema| {
        let mut lines: Vec<String> = summary(s).lines().map(str::to_string).collect();
        lines.sort();
        lines
    };
    assert_eq!(sorted(&one), sorted(&four));
}

#[test]
fn headerless_one_file_export_reads_the_same() {
    let text = demo("workshop-sqlite.csv");
    let without_header = text.split_once('\n').unwrap().1.to_string();
    let mut files = ExportFiles::default();
    assert_eq!(files.add(without_header), Ok(ExportKind::Combined));
    assert_eq!(
        summary(&build_schema(&files)),
        fixture("workshop-sqlite.expected.txt")
    );
}

#[test]
fn demo_facts_used_by_later_milestones() {
    let target = load(&["catalogue-postgresql.csv"]);
    let asset = target.table("public.asset").expect("asset table");
    let parent = asset
        .foreign_keys
        .iter()
        .find(|f| f.columns == ["parent_asset_id"])
        .expect("parent link");
    // Deliberate mistake in the demo: the parent link points to `location`
    // and was added with NOT VALID.
    assert_eq!(parent.references, "public.location");
    assert!(parent.not_valid);

    let source = load(&["workshop-sqlite.csv"]);
    assert_eq!(source.dbms, "sqlite");
    let run = source.table("main.run").expect("run table");
    let machine_link = run
        .foreign_keys
        .iter()
        .find(|f| f.columns == ["machine_id"])
        .unwrap();
    assert_eq!(machine_link.references, "main.machine");
    let partial = run
        .indexes
        .iter()
        .find(|i| i.name.as_deref() == Some("ix_run_machine"));
    assert!(partial.is_some());
}
