//! Inputs shared by the parity tests of matching and the integration plan.

use std::fs;
use std::path::PathBuf;

use schema_bridge_core::{Overrides, TableChoice};

pub fn read(path: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// The overrides of `fixtures/plan/sample-overrides.json`.
pub fn sample_overrides() -> Overrides {
    let mut o = Overrides::default();
    o.tables.insert(
        "main.booking".into(),
        TableChoice::Target("public.session".into()),
    );
    o.tables
        .insert("main.operator".into(), TableChoice::NoTarget);
    let mut column = |t: &str, c: &str, to: Option<&str>| {
        o.columns
            .insert((t.into(), c.into()), to.map(str::to_string));
    };
    column("main.machine", "room", Some("location_id"));
    column("main.machine", "notes", None);
    column("main.machine", "power_kw", Some("description"));
    column("main.run", "id", None);
    o
}

/// Compares line by line, so a difference names its line.
pub fn assert_same(actual: &str, expected: &str) {
    for (i, (a, b)) in actual.lines().zip(expected.lines()).enumerate() {
        assert_eq!(a, b, "line {} differs", i + 1);
    }
    assert_eq!(actual.lines().count(), expected.lines().count(), "lines");
}
