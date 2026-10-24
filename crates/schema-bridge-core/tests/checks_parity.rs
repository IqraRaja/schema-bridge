//! Problem detection must give the same findings as the prototype's
//! `dbChecks` on the sample exports. The expected files were produced by
//! running the prototype on the same files (see docs/decisions.md, decision 15).

use std::fs;
use std::path::PathBuf;

use schema_bridge_core::checks::RULES;
use schema_bridge_core::{ExportFiles, Finding, Severity, build_schema, check_schema};

fn demo(path: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../demo")
        .join(path);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn expected(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn findings(files: &[&str]) -> Vec<Finding> {
    let mut export = ExportFiles::default();
    for f in files {
        export.add(demo(f)).unwrap();
    }
    check_schema(&build_schema(&export))
}

/// One line per finding, in the format of the generating script.
fn lines(findings: &[Finding]) -> String {
    let e = |s: &str| s.replace('\\', "\\\\").replace('\n', "\\n");
    findings
        .iter()
        .map(|f| {
            [
                f.severity.label(),
                &f.area,
                &f.object,
                &f.title,
                &f.explanation,
                &f.fix,
            ]
            .map(e)
            .join(" | ")
                + "\n"
        })
        .collect()
}

fn assert_same(actual: &str, expected: &str) {
    for (i, (a, b)) in actual.lines().zip(expected.lines()).enumerate() {
        assert_eq!(a, b, "finding {} differs", i + 1);
    }
    assert_eq!(
        actual.lines().count(),
        expected.lines().count(),
        "number of findings"
    );
}

#[test]
fn workshop_sqlite_matches_the_prototype() {
    let actual = lines(&findings(&["workshop-sqlite.csv"]));
    assert_same(&actual, &expected("workshop-sqlite.checks.expected.txt"));
}

#[test]
fn catalogue_postgresql_matches_the_prototype() {
    let actual = lines(&findings(&["catalogue-postgresql.csv"]));
    assert_same(
        &actual,
        &expected("catalogue-postgresql.checks.expected.txt"),
    );
}

#[test]
fn catalogue_four_file_export_matches_the_prototype() {
    let actual = lines(&findings(&[
        "catalogue-postgresql-4files/columns.csv",
        "catalogue-postgresql-4files/constraints.csv",
        "catalogue-postgresql-4files/indexes.csv",
        "catalogue-postgresql-4files/rowcounts.csv",
    ]));
    assert_same(
        &actual,
        &expected("catalogue-postgresql-4files.checks.expected.txt"),
    );
}

#[test]
fn the_deliberate_wrong_link_in_the_sample_is_the_only_blocking_finding() {
    let found = findings(&["catalogue-postgresql.csv"]);
    let blocking: Vec<&Finding> = found
        .iter()
        .filter(|f| f.severity == Severity::Blocking)
        .collect();
    assert_eq!(blocking.len(), 1);
    assert_eq!(blocking[0].object, "asset.parent_asset_id → location");
}

/// A made-up database (tests/fixtures/checks-coverage/library.sql) that
/// triggers the checks the samples do not: secrets, lists of IDs, duplicate
/// indexes, ID generators, misleading names, quoted constraint names and more.
#[test]
fn coverage_fixture_matches_the_prototype() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/checks-coverage/library-postgresql.csv");
    let mut export = ExportFiles::default();
    export.add(fs::read_to_string(p).unwrap()).unwrap();
    let actual = lines(&check_schema(&build_schema(&export)));
    assert_same(
        &actual,
        &expected("checks-coverage/library-postgresql.checks.expected.txt"),
    );
}

/// Every finding carries a rule id from the catalogue, and each check uses
/// the right one.
#[test]
fn findings_carry_rule_ids_from_the_catalogue() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/checks-coverage/library-postgresql.csv");
    let mut export = ExportFiles::default();
    export.add(fs::read_to_string(p).unwrap()).unwrap();
    let mut found = check_schema(&build_schema(&export));
    found.extend(findings(&["catalogue-postgresql.csv"]));
    for f in &found {
        assert!(RULES.contains(&f.rule), "unknown rule {}", f.rule);
    }
    let rule_of = |title: &str| found.iter().find(|f| f.title == title).unwrap().rule;
    assert_eq!(rule_of("Table has no primary key"), "KEY-MISSING-PK");
    assert_eq!(
        rule_of("Link probably points to the wrong table"),
        "LINK-WRONG-TARGET"
    );
    assert_eq!(rule_of("3 links have no index"), "PERF-FK-NO-INDEX");
    assert_eq!(rule_of("7 tables are empty"), "CONTENT-EMPTY");
    assert_eq!(rule_of("Row counts unknown"), "CONTENT-EMPTY");
    assert_eq!(rule_of("Descriptions on 3% of columns"), "DOC-COVERAGE");
    assert_eq!(rule_of("Sensitive data"), "SEC-SENSITIVE");
    // Together, the coverage fixture and the Catalogue sample trigger every
    // rule.
    let mut used: Vec<&str> = found.iter().map(|f| f.rule).collect();
    used.sort();
    used.dedup();
    assert_eq!(used.len(), RULES.len());
}
