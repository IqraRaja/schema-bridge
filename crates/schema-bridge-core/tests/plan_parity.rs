//! Integration findings, load order and draft SQL must match the
//! prototype's `integrationChecks`, `loadOrder` and plan script on the
//! sample pair. Same expected files as `matching_parity.rs`; this test
//! compares the part from `# integration` on (docs/decisions.md, decision 19).

mod common;

use common::{assert_same, read, sample_overrides};
use schema_bridge_core::Overrides;
use schema_bridge_core::workspace::Workspace;

/// The sample workspace with the given names, word list and choices.
fn workspace(names: (&str, &str), word_list: bool, overrides: Overrides) -> Workspace {
    let mut ws = Workspace::new();
    ws.load_samples();
    ws.source.name = names.0.into();
    ws.target.name = names.1.into();
    if !word_list {
        ws.word_list.clear();
    }
    ws.overrides = overrides;
    ws
}

/// Prints the plan in the format of the generating script.
fn summary(ws: &Workspace) -> String {
    let e = |s: &str| s.replace('\\', "\\\\").replace('\n', "\\n");
    let a = ws.analysis().expect("both loaded");
    let mut out = vec!["# integration".to_string()];
    for f in &a.integration {
        out.push(
            [
                f.severity.label(),
                &f.area,
                &f.object,
                &f.title,
                &f.explanation,
                &f.fix,
            ]
            .map(e)
            .join(" | "),
        );
    }
    out.push("# order".into());
    for s in &a.load_order {
        let from = if s.from.is_empty() {
            "-".to_string()
        } else {
            s.from.join(", ")
        };
        out.push(format!(
            "{} | {from} | {}",
            s.table,
            if s.cycle { "cycle" } else { "-" }
        ));
    }
    out.push("# sql".into());
    out.join("\n") + "\n" + &ws.draft_sql(&a)
}

fn expected(file: &str) -> String {
    let text = read(&format!("tests/fixtures/plan/{file}"));
    let at = text.find("# integration").expect("integration section");
    text[at..].to_string()
}

const SAMPLE_NAMES: (&str, &str) = ("Workshop (sample)", "Catalogue (sample)");

#[test]
fn sample_pair_with_its_word_list_matches_the_prototype() {
    let ws = workspace(SAMPLE_NAMES, true, Overrides::default());
    assert_same(&summary(&ws), &expected("sample.expected.txt"));
}

#[test]
fn sample_pair_with_manual_choices_matches_the_prototype() {
    let ws = workspace(SAMPLE_NAMES, true, sample_overrides());
    assert_same(&summary(&ws), &expected("sample-overrides.expected.txt"));
}

#[test]
fn sample_pair_without_a_word_list_matches_the_prototype() {
    let ws = workspace(("Workshop", "Catalogue"), false, Overrides::default());
    assert_same(&summary(&ws), &expected("sample-no-word-list.expected.txt"));
}

#[test]
fn integration_findings_carry_their_rule_ids() {
    let ws = workspace(SAMPLE_NAMES, true, sample_overrides());
    let a = ws.analysis().unwrap();
    let rules: Vec<&str> = a.integration.iter().map(|f| f.rule).collect();
    assert_eq!(
        &rules[..4],
        [
            "MAP-REQUIRED-LINK",
            "MAP-REQUIRED-VALUE",
            "MAP-REQUIRED-VALUE",
            "MAP-DUPLICATES"
        ]
    );
    assert!(
        rules
            .iter()
            .all(|r| schema_bridge_core::integration::INTEGRATION_RULES.contains(r))
    );
}
