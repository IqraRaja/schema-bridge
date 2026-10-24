//! Reviewing matches on the sample pair: confidence and reasons, the review
//! states the Matching stage filters by, accepting, removing and overriding
//! matches by hand, searching and sorting.

use schema_bridge_core::matching::{
    ColumnFilter, ColumnHow, ColumnReview, MatchKind, Matching, Review, TableFilter, TableSort,
};
use schema_bridge_core::workspace::Workspace;
use schema_bridge_core::{Fit, TableChoice, TableMapping};

fn sample() -> Workspace {
    let mut ws = Workspace::new();
    ws.load_samples();
    ws
}

fn matching(ws: &Workspace) -> Matching {
    ws.matching().expect("both loaded")
}

fn row<'a>(m: &'a TableMapping, column: &str) -> &'a schema_bridge_core::matching::ColumnRow {
    m.rows.iter().find(|r| r.source.name == column).unwrap()
}

fn names(list: &[&schema_bridge_core::TableMatch]) -> Vec<String> {
    list.iter()
        .map(|m| schema_bridge_core::table_name(&m.source).to_string())
        .collect()
}

#[test]
fn automatic_matches_carry_their_confidence_and_reason() {
    let ws = sample();
    let m = matching(&ws);

    // operator → person scores 60%: matched, but uncertain.
    let operator = m.table("main.operator").unwrap();
    assert_eq!(operator.kind, MatchKind::Automatic);
    assert!(operator.is_uncertain());
    assert!(operator.reason().contains("60% similar"));
    // machine → asset scores 76%: confident enough.
    assert!(!m.table("main.machine").unwrap().is_uncertain());
    assert!(
        m.table("main.machine_function")
            .unwrap()
            .reason()
            .contains("word list")
    );
    assert!(
        m.table("main.booking")
            .unwrap()
            .reason()
            .contains("skip rule")
    );

    let machine = m.mapping("main.machine").unwrap();
    // id → uuid: the names agree (id = uuid) but the types do not fit.
    let id = row(machine, "id");
    let e = id.evidence.expect("scored");
    assert_eq!((e.names, e.type_bonus), (1.0, -0.25));
    assert!((e.confidence() - 0.75).abs() < 1e-9);
    assert!(id.is_uncertain());
    assert_eq!(id.reason(), "The names match, but the types do not fit.");
    // label → name through the word list word "label = name", types fit.
    let label = row(machine, "label");
    assert!(!label.is_uncertain());
    assert_eq!(label.reason(), "The names match, the types fit.");
    // serial_no = code is a whole-name rule in the word list.
    assert_eq!(
        row(machine, "serial_no").reason(),
        "Your word list says these column names mean the same."
    );
    assert_eq!(
        row(machine, "room").reason(),
        "No target column matched well enough."
    );
    let operator_map = m.mapping("main.operator").unwrap();
    // `password_hash` does not end in a secret word, so it is simply unmatched.
    assert_eq!(
        row(operator_map, "password_hash").reason(),
        "No target column matched well enough."
    );
}

#[test]
fn type_fit_of_every_mapped_column_is_classified_and_explained() {
    let ws = sample();
    let m = matching(&ws);
    let run = m.mapping("main.run").unwrap();
    let started = row(run, "started").compatibility.as_ref().unwrap();
    assert_eq!(started.fit, Fit::Convert);
    assert_eq!(started.fit.label(), "Requires conversion");
    assert!(started.note.contains("time zone"));
    let id = row(run, "id").compatibility.as_ref().unwrap();
    assert_eq!(id.fit.label(), "Incompatible");
    let title = row(run, "title").compatibility.as_ref().unwrap();
    assert_eq!(title.fit, Fit::Safe);
}

#[test]
fn review_states_count_the_tables() {
    let ws = sample();
    let m = matching(&ws);
    assert_eq!(m.count(Review::Matched), 4);
    assert_eq!(m.count(Review::Stays), 1);
    assert_eq!(m.count(Review::Unmatched), 0);
    assert_eq!(m.count(Review::Manual), 0);
    // Every matched table has an uncertain id → uuid column.
    assert_eq!(m.count(Review::Uncertain), 4);
}

#[test]
fn accepting_removing_and_overriding_by_hand() {
    let mut ws = sample();

    // Accept the uncertain id → uuid: it becomes the user's choice.
    ws.set_column_choice("main.machine", "id", Some(Some("uuid".into())));
    let m = matching(&ws);
    let id = row(m.mapping("main.machine").unwrap(), "id");
    assert!(id.manual && !id.is_uncertain());
    assert_eq!(id.how, Some(ColumnHow::User));
    assert_eq!(id.reason(), "Chosen by you.");
    assert_eq!(m.count(Review::Manual), 1);
    assert_eq!(m.count(Review::Uncertain), 3);

    // Remove a mapping: the column is no longer copied, by the user's choice.
    ws.set_column_choice("main.machine", "serial_no", Some(None));
    let m = matching(&ws);
    let machine = m.mapping("main.machine").unwrap();
    let serial = row(machine, "serial_no");
    assert!(serial.manual && serial.target.is_none());
    assert_eq!(serial.reason(), "You chose not to copy this column.");
    // The target's required `code` column now has no source.
    assert!(machine.required.iter().any(|r| r.column.name == "code"));

    // Override an automatic table match, then accept an uncertain one.
    ws.set_table_choice(
        "main.booking",
        Some(TableChoice::Target("public.session".into())),
    );
    ws.set_table_choice(
        "main.operator",
        Some(TableChoice::Target("public.person".into())),
    );
    let m = matching(&ws);
    assert_eq!(m.table("main.booking").unwrap().kind, MatchKind::Manual);
    let operator = m.table("main.operator").unwrap();
    assert_eq!(operator.kind, MatchKind::Manual);
    assert!(!operator.is_uncertain());
    assert_eq!(operator.reason(), "Matched by your choice.");

    // Undo: back to the tool's decision.
    ws.set_column_choice("main.machine", "serial_no", None);
    ws.set_table_choice("main.operator", None);
    let m = matching(&ws);
    assert_eq!(
        row(m.mapping("main.machine").unwrap(), "serial_no")
            .target
            .as_ref()
            .unwrap()
            .name,
        "code"
    );
    assert!(m.table("main.operator").unwrap().is_uncertain());
}

#[test]
fn the_table_search_looks_at_table_and_column_names_on_both_sides() {
    let ws = sample();
    let m = matching(&ws);
    let source = ws.source.schema.as_ref().unwrap();
    let find = |text: &str, review: Option<Review>| {
        names(&m.filtered(
            source,
            &TableFilter {
                text: text.into(),
                review,
            },
            TableSort::Name,
        ))
    };
    assert_eq!(find("", None).len(), 5);
    // A source column name.
    assert_eq!(find("SERIAL", None), ["machine"]);
    // A target table name.
    assert_eq!(find("session", None), ["run"]);
    // A target column name.
    assert_eq!(find("time_start", None), ["run"]);
    assert_eq!(find("", Some(Review::Stays)), ["booking"]);
    assert_eq!(
        find("machine", Some(Review::Matched)),
        ["machine", "machine_function", "run"]
    );
    assert!(find("nothing like this", None).is_empty());
}

#[test]
fn sorting_puts_what_needs_attention_first() {
    let ws = sample();
    let m = matching(&ws);
    let source = ws.source.schema.as_ref().unwrap();
    let all = TableFilter::default();
    let by_attention = names(&m.filtered(source, &all, TableSort::Attention));
    assert_eq!(by_attention.last().unwrap(), "booking");
    let by_size = names(&m.filtered(source, &all, TableSort::Size));
    assert_eq!(by_size[0], "run"); // 9 columns
    assert_eq!(
        names(&m.filtered(source, &all, TableSort::Name))[0],
        "booking"
    );
}

#[test]
fn the_column_filter_finds_columns_by_name_description_and_state() {
    let mut ws = sample();
    ws.set_column_choice("main.run", "id", Some(None));
    let m = matching(&ws);
    let run = m.mapping("main.run").unwrap();
    let shown = |text: &str, review: Option<ColumnReview>| -> Vec<String> {
        let f = ColumnFilter {
            text: text.into(),
            review,
        };
        run.rows
            .iter()
            .filter(|r| f.matches(r))
            .map(|r| r.source.name.clone())
            .collect()
    };
    assert_eq!(shown("time_", None), ["started", "ended"]);
    assert_eq!(shown("", Some(ColumnReview::Manual)), ["id"]);
    assert_eq!(
        shown("", Some(ColumnReview::Mapped)),
        ["title", "started", "ended"]
    );
    assert_eq!(
        shown("settings", Some(ColumnReview::NotMapped)),
        ["settings_xml", "settings_json"]
    );
    assert!(shown("", Some(ColumnReview::Uncertain)).is_empty());

    // Descriptions are searched too.
    let machine = m.mapping("main.machine").unwrap();
    let f = ColumnFilter {
        text: "printed on".into(),
        review: None,
    };
    let hits: Vec<_> = machine
        .rows
        .iter()
        .filter(|r| f.matches(r))
        .map(|r| r.source.name.as_str())
        .collect();
    assert_eq!(hits, ["serial_no"]); // target `code` is described as "printed on the asset label"
}
