//! Matching must give the same table matches and column mappings as the
//! prototype's `matchTables` and `mapColumns` on the sample pair. The
//! expected files in `tests/fixtures/plan/` were produced by running the
//! prototype on the same exports and word list (docs/decisions.md,
//! decision 17); this test compares the part before `# integration`.

mod common;

use common::{assert_same as same, read, sample_overrides};
use schema_bridge_core::matching::{MatchKind, Matching};
use schema_bridge_core::{ExportFiles, Overrides, Schema, WordList, build_schema, match_schemas};

fn schema(demo_file: &str) -> Schema {
    let mut files = ExportFiles::default();
    files.add(read(&format!("../../demo/{demo_file}"))).unwrap();
    build_schema(&files)
}

/// Prints matches and mappings in the format of the generating script.
fn summary(m: &Matching) -> String {
    let e = |s: &str| s.replace('\\', "\\\\").replace('\n', "\\n");
    let f4 = |x: Option<f64>| x.map_or("-".to_string(), |x| format!("{x:.4}"));
    let mut out = vec!["# matches".to_string()];
    for t in &m.tables {
        let how = match t.kind {
            MatchKind::Automatic => "automatic",
            MatchKind::WordList => "word list",
            MatchKind::Manual => "manual",
            MatchKind::NoTarget => "none",
            MatchKind::Stays => "skip",
        };
        let by = if t.kind == MatchKind::Automatic {
            "-"
        } else {
            t.decided_by.label()
        };
        out.push(
            [
                t.source.clone(),
                how.into(),
                by.into(),
                t.target.clone().unwrap_or("-".into()),
                f4(t.score),
                t.candidate
                    .as_ref()
                    .map_or("-".into(), |c| c.target.clone()),
                f4(t.candidate.as_ref().map(|c| c.score)),
            ]
            .join(" | "),
        );
    }
    for p in &m.mappings {
        out.push(format!("# columns {} -> {}", p.source, p.target));
        for r in &p.rows {
            let notes: Vec<String> = r
                .notes
                .iter()
                .map(|n| format!("{}: {}", n.severity.label(), n.text))
                .collect();
            let fit = r
                .compatibility
                .as_ref()
                .map_or("-".to_string(), |c| format!("{:?}", c.fit).to_lowercase());
            out.push(
                [
                    r.source.name.clone(),
                    r.target.as_ref().map_or("-".into(), |c| c.name.clone()),
                    r.how.map_or("-", |h| h.label()).into(),
                    r.status.code().into(),
                    fit,
                    notes.join(" ; "),
                ]
                .map(|s| e(&s))
                .join(" | "),
            );
        }
        let required: Vec<String> = p
            .required
            .iter()
            .map(|r| match &r.link_to {
                Some(k) => format!("{} -> {k}", r.column.name),
                None => r.column.name.clone(),
            })
            .collect();
        out.push(format!("required: {}", required.join(", ")));
        out.push(format!("trace: {}", p.trace.as_deref().unwrap_or("-")));
    }
    out.join("\n") + "\n"
}

fn assert_same(actual: &str, expected_file: &str) {
    let expected = read(&format!("tests/fixtures/plan/{expected_file}"));
    same(actual, expected.split("# integration").next().unwrap());
}

fn sample_words() -> WordList {
    WordList::parse(&read("../../demo/sample-word-list.txt"))
}

#[test]
fn sample_pair_with_its_word_list_matches_the_prototype() {
    let m = match_schemas(
        &schema("workshop-sqlite.csv"),
        &schema("catalogue-postgresql.csv"),
        &sample_words(),
        &Overrides::default(),
    );
    assert_same(&summary(&m), "sample.expected.txt");
}

#[test]
fn sample_pair_with_manual_choices_matches_the_prototype() {
    let m = match_schemas(
        &schema("workshop-sqlite.csv"),
        &schema("catalogue-postgresql.csv"),
        &sample_words(),
        &sample_overrides(),
    );
    assert_same(&summary(&m), "sample-overrides.expected.txt");
}

#[test]
fn sample_pair_without_a_word_list_matches_the_prototype() {
    let m = match_schemas(
        &schema("workshop-sqlite.csv"),
        &schema("catalogue-postgresql.csv"),
        &WordList::default(),
        &Overrides::default(),
    );
    assert_same(&summary(&m), "sample-no-word-list.expected.txt");
}

#[test]
fn the_sample_word_list_is_fully_understood() {
    let words = sample_words();
    assert!(words.unrecognised.is_empty(), "{:?}", words.unrecognised);
    assert_eq!(words.rule_count(), 12);
}
