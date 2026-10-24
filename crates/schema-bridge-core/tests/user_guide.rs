//! The walk-through in docs/user-guide.md quotes concrete results for the
//! sample schemas. This test checks every one of them, so the guide stays
//! true when the code changes. If it fails after an intended change, update
//! the guide together with the expected values here.

use schema_bridge_core::matching::{MatchKind, Review};
use schema_bridge_core::report::summary;
use schema_bridge_core::workspace::{FindingFilter, Side, Workspace};
use schema_bridge_core::{Severity, TableChoice, table_name};

#[test]
fn the_walk_through_in_the_user_guide_is_accurate() {
    let mut ws = Workspace::new();
    ws.load_samples();

    // 1. Load: SQLite with 5 tables, PostgreSQL with 7.
    assert_eq!(ws.source.schema.as_ref().unwrap().dbms, "sqlite");
    assert_eq!(ws.source.schema.as_ref().unwrap().tables.len(), 5);
    assert_eq!(ws.target.schema.as_ref().unwrap().tables.len(), 7);
    assert_eq!(ws.source.name, "Workshop (sample)");

    // 2. Problems: 16 findings, 1 blocking on asset.parent_asset_id.
    let findings = ws.findings();
    assert_eq!(findings.len(), 16);
    let blocking: Vec<_> = findings
        .iter()
        .filter(|(_, f)| f.severity == Severity::Blocking)
        .collect();
    assert_eq!(blocking.len(), 1);
    assert!(blocking[0].1.object.starts_with("asset.parent_asset_id"));
    let count = |filter: FindingFilter| {
        findings
            .iter()
            .filter(|(s, f)| filter.matches(*s, f))
            .count()
    };
    let serial = FindingFilter {
        text: "serial".into(),
        ..Default::default()
    };
    assert_eq!(count(serial.clone()), 1);
    assert!(
        findings
            .iter()
            .any(|(s, f)| serial.matches(*s, f) && f.object == "machine.serial_no")
    );
    let implicit = FindingFilter {
        rule: Some("LINK-IMPLICIT"),
        ..Default::default()
    };
    let hit: Vec<_> = findings
        .iter()
        .filter(|(s, f)| implicit.matches(*s, f))
        .collect();
    assert_eq!(hit.len(), 1);
    assert_eq!(hit[0].0, Side::Source);
    assert_eq!(hit[0].1.object, "machine_function.machine_id");

    // 3. Matching: 4 of 5 tables have a place; booking stays; machine → asset 76 %.
    let a = ws.analysis().unwrap();
    let m = &a.matching;
    assert_eq!(m.count(Review::Matched), 4);
    assert_eq!(m.table("main.booking").unwrap().kind, MatchKind::Stays);
    let machine = m.table("main.machine").unwrap();
    assert_eq!(machine.target.as_deref(), Some("public.asset"));
    assert_eq!(format!("{:.0}", machine.score.unwrap() * 100.0), "76");
    let asset = m.mapping("main.machine").unwrap();
    assert!(
        asset
            .required
            .iter()
            .any(|r| r.column.name == "asset_type_id")
    );
    let id = asset.rows.iter().find(|r| r.source.name == "id").unwrap();
    assert_eq!(
        format!("{:.0}", id.evidence.unwrap().confidence() * 100.0),
        "75"
    );
    assert_eq!(
        id.compatibility.as_ref().unwrap().fit.label(),
        "Incompatible"
    );
    let operator = m.table("main.operator").unwrap();
    assert_eq!(format!("{:.0}", operator.score.unwrap() * 100.0), "60");
    assert!(operator.is_uncertain());

    // Removing id → uuid and undoing it.
    ws.set_column_choice("main.machine", "id", Some(None));
    let removed = ws.matching().unwrap();
    let id = removed
        .mapping("main.machine")
        .unwrap()
        .rows
        .iter()
        .find(|r| r.source.name == "id")
        .unwrap()
        .clone();
    assert_eq!(id.reason(), "You chose not to copy this column.");
    ws.set_column_choice("main.machine", "id", None);
    // Accepting operator.
    ws.set_table_choice(
        "main.operator",
        Some(TableChoice::Target("public.person".into())),
    );
    assert!(
        !ws.matching()
            .unwrap()
            .table("main.operator")
            .unwrap()
            .is_uncertain()
    );
    ws.set_table_choice("main.operator", None);

    // 4. Integration issues: 11, the missing asset_type_id first.
    assert_eq!(a.integration.len(), 11);
    assert_eq!(a.integration[0].severity, Severity::Blocking);
    assert!(a.integration[0].title.contains("asset_type_id"));

    // 5. Plan: the load order.
    let order: Vec<(&str, Vec<String>)> = a
        .load_order
        .iter()
        .map(|s| (table_name(&s.table), s.from.clone()))
        .collect();
    assert_eq!(
        order,
        [
            ("asset_type", vec![]),
            ("person", vec!["operator".to_string()]),
            ("session", vec!["run".to_string()]),
            ("asset", vec!["machine".to_string()]),
            ("capability", vec!["machine_function".to_string()]),
        ]
    );

    // 6. Draft SQL has TODOs. 7. Report: 4 items to resolve.
    assert!(ws.draft_sql(&a).contains("TODO"));
    let s = summary(&ws, Some(&a));
    let open = s
        .unresolved
        .iter()
        .filter(|u| u.kind == "Fix first" || u.kind == "Decide")
        .count();
    assert_eq!(open, 4);
}
