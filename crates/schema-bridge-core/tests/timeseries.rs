//! Time-series schemas (Milestone 8): the InfluxDB schema export through the
//! whole pipeline, from import to checks, matching, integration findings,
//! draft SQL, the report and the saved comparison. The sample pair is
//! demo/plant-influxdb.csv (source) and demo/readings-postgresql.csv
//! (target); docs/time-series.md quotes the results checked here.

use schema_bridge_core::influx::retention_summary;
use schema_bridge_core::integration::integration_findings;
use schema_bridge_core::matching::{ColumnStatus, MatchKind, Overrides, WordList, match_schemas};
use schema_bridge_core::model::{ColumnRole, TableKind};
use schema_bridge_core::report::{report_markdown, summary};
use schema_bridge_core::sql::{draft_available, draft_sql};
use schema_bridge_core::workspace::{Side, Workspace};
use schema_bridge_core::workspace_file::{open_workspace, save_workspace};
use schema_bridge_core::{Fit, Schema, Severity, check_schema, table_name};

const HEADER: &str = "record_type,bucket,retention,measurement,key,data_type,series,dbms\n";
const PG_HEADER: &str = "record_type,table_schema,table_name,object_type,ordinal_position,column_name,data_type,is_nullable,column_default,table_comment,column_comment,constraint_name,constraint_type,definition,ref_schema,ref_table,indexname,indexdef,estimated_rows,triggers,dbms\n";

/// Builds a schema through the normal file import.
fn load(text: &str) -> Schema {
    let mut ws = Workspace::new();
    ws.add_file(Side::Source, "export.csv", text.to_string());
    ws.source
        .schema
        .unwrap_or_else(|| panic!("not imported: {:?}", ws.source.notice))
}

fn influx(rows: &str) -> Schema {
    load(&format!("{HEADER}{rows}"))
}

/// A PostgreSQL table `public.boiler(recorded_at, site, unit_id,
/// temperature)`, optionally with a unique rule on the point identity.
fn pg_boiler(unique: bool) -> Schema {
    let mut text = PG_HEADER.to_string();
    for (i, (name, ty)) in [
        ("recorded_at", "timestamp with time zone"),
        ("site", "text"),
        ("unit_id", "text"),
        ("temperature", "double precision"),
    ]
    .iter()
    .enumerate()
    {
        text.push_str(&format!(
            "column,public,boiler,table,{},{name},{ty},false,,,,,,,,,,,,,postgresql\n",
            i + 1
        ));
    }
    if unique {
        text.push_str("constraint,public,boiler,,,,,,,,,boiler_point_key,UNIQUE,\"UNIQUE (site, unit_id, recorded_at)\",,,,,,,postgresql\n");
    }
    load(&text)
}

fn rules(findings: &[schema_bridge_core::Finding]) -> Vec<&'static str> {
    findings.iter().map(|f| f.rule).collect()
}

fn analyse(source: &Schema, target: &Schema, words: &str) -> Vec<schema_bridge_core::Finding> {
    let m = match_schemas(
        source,
        target,
        &WordList::parse(words),
        &Overrides::default(),
    );
    integration_findings(source, target, "S", "T", &m)
}

#[test]
fn the_sample_pair_runs_through_the_whole_pipeline() {
    let mut ws = Workspace::new();
    ws.load_timeseries_samples();
    assert!(ws.using_samples);
    assert_eq!(ws.source.name, "Plant (sample)");
    assert_eq!(ws.target.name, "Readings (sample)");
    let source = ws.source.schema.clone().unwrap();
    let target = ws.target.schema.clone().unwrap();
    assert_eq!(source.dbms, "influxdb");
    assert!(source.is_time_series() && !target.is_time_series());
    let st = source.time_series_stats();
    assert_eq!(
        (st.measurements, st.tags, st.fields, st.series),
        (3, 6, 9, Some(622))
    );
    assert_eq!(
        retention_summary(&source).as_deref(),
        Some("plant: 30 days")
    );

    // Problems: only time-series rules in the source, none of the relational
    // rules (no primary key, no links) fire on measurements.
    let source_rules: Vec<&str> = ws
        .findings()
        .into_iter()
        .filter(|(side, _)| *side == Side::Source)
        .map(|(_, f)| f.rule)
        .collect();
    assert_eq!(
        source_rules,
        [
            "TS-FIELD-TYPE-CONFLICT",
            "TS-FIELD-TYPE-UNKNOWN",
            "TS-UNSIGNED-FIELD"
        ]
    );

    // Matching: boiler → readings through the word list, pump →
    // pump_readings by name, energy_meter has no place.
    let a = ws.analysis().unwrap();
    let kind = |m: &str| a.matching.table(m).unwrap();
    assert_eq!(kind("plant.boiler").kind, MatchKind::WordList);
    assert_eq!(
        kind("plant.boiler").target.as_deref(),
        Some("public.readings")
    );
    assert_eq!(kind("plant.pump").kind, MatchKind::Automatic);
    assert_eq!(
        kind("plant.pump").target.as_deref(),
        Some("public.pump_readings")
    );
    assert_eq!(kind("plant.energy_meter").kind, MatchKind::NoTarget);

    let boiler = a.matching.mapping("plant.boiler").unwrap();
    let row = |m: &schema_bridge_core::TableMapping, c: &str| {
        m.rows.iter().find(|r| r.source.name == c).unwrap().clone()
    };
    let time = row(boiler, "time");
    assert_eq!(time.target.as_ref().unwrap().name, "recorded_at");
    assert_eq!(time.compatibility.as_ref().unwrap().fit, Fit::Loss);
    assert!(time.compatibility.unwrap().note.contains("nanosecond"));
    assert_eq!(
        row(boiler, "firmware_version").status,
        ColumnStatus::NotCopied
    );
    assert_eq!(row(boiler, "running").status, ColumnStatus::Copy);
    let pump = a.matching.mapping("plant.pump").unwrap();
    let speed = row(pump, "speed_rpm").compatibility.unwrap();
    assert_eq!(speed.fit, Fit::Loss);
    assert!(speed.note.contains("integer | float"));

    // Integration: readings has no unique rule on (site, unit_id,
    // recorded_at), pump_readings has one; both have no retention period.
    let found: Vec<(&str, &str)> = a
        .integration
        .iter()
        .filter(|f| f.area == "Time series")
        .map(|f| (f.rule, f.object.as_str()))
        .collect();
    assert_eq!(
        found,
        [
            ("MAP-TS-SERIES-KEY", "boiler → readings"),
            ("MAP-TS-RETENTION", "boiler → readings"),
            ("MAP-TS-RETENTION", "pump → pump_readings"),
        ]
    );
    let series_key = a
        .integration
        .iter()
        .find(|f| f.rule == "MAP-TS-SERIES-KEY")
        .unwrap();
    assert!(series_key.fix.contains("does not add it automatically"));
    // A measurement has no source ID to keep.
    assert!(!rules(&a.integration).contains(&"MAP-NO-SOURCE-ID"));
    assert!(rules(&a.integration).contains(&"MAP-NO-TARGET"));

    // Draft SQL: CSV staging, because there is no foreign data wrapper.
    let sql = ws.draft_sql(&a);
    assert!(ws.sql_available());
    assert!(sql.contains("No foreign data wrapper is known"));
    assert!(!sql.contains("IMPORT FOREIGN SCHEMA") && !sql.contains("CREATE SERVER"));
    assert!(sql.contains("CREATE TABLE staging.boiler ("));
    assert!(sql.contains("-- \\copy staging.pump FROM 'pump.csv' WITH (FORMAT csv, HEADER)"));
    // A field of unknown or conflicting type is staged as text.
    assert!(sql.contains("  firmware_version text") && sql.contains("  speed_rpm text"));
    assert!(sql.contains("INSERT INTO public.readings ("));
    assert!(sql.find("pump → pump_readings").is_some());
}

#[test]
fn time_series_checks_fire_only_on_real_risks() {
    let clean = influx(
        "bucket,b,INF,,,,,influxdb\n\
         measurement,b,,cpu,,,40,influxdb\n\
         tag,b,,cpu,host,,,influxdb\n\
         field,b,,cpu,usage,float,,influxdb\n",
    );
    assert!(
        check_schema(&clean).is_empty(),
        "{:?}",
        check_schema(&clean)
    );

    let risky = influx(
        "bucket,b,7d,,,,,influxdb\n\
         measurement,b,,empty,,,3,influxdb\n\
         tag,b,,empty,host,,,influxdb\n\
         measurement,b,,http,,,250000,influxdb\n\
         tag,b,,http,host,,,influxdb\n\
         field,b,,http,host,string,,influxdb\n\
         field,b,,http,bytes,unsigned,,influxdb\n\
         measurement,b,,trace,,,10,influxdb\n\
         tag,b,,trace,request_id,,,influxdb\n\
         field,b,,trace,ms,integer,,influxdb\n\
         field,b,,trace,ms,float,,influxdb\n\
         field,b,,trace,note,,,influxdb\n",
    );
    let f = check_schema(&risky);
    let got: Vec<(&str, Severity, &str)> = f
        .iter()
        .map(|f| (f.rule, f.severity, f.object.as_str()))
        .collect();
    for expected in [
        ("TS-MEASUREMENT-NO-FIELDS", Severity::High, "empty"),
        ("TS-HIGH-CARDINALITY", Severity::Medium, "http"),
        ("TS-TAG-FIELD-SAME-KEY", Severity::Medium, "http.host_1"),
        ("TS-UNSIGNED-FIELD", Severity::Info, "http.bytes"),
        ("TS-HIGH-CARDINALITY", Severity::Low, "trace.request_id"),
        ("TS-FIELD-TYPE-CONFLICT", Severity::High, "trace.ms"),
        ("TS-FIELD-TYPE-UNKNOWN", Severity::Medium, "trace.note"),
    ] {
        assert!(got.contains(&expected), "missing {expected:?} in {got:?}");
    }
    assert_eq!(got.len(), 7, "{got:?}");
    assert!(f.iter().all(|f| f.area == "Time series"));
    // Measurements never trigger relational rules.
    assert!(f.iter().all(|f| f.rule.starts_with("TS-")));
}

#[test]
fn tags_match_tags_and_fields_match_fields_between_two_buckets() {
    let a = influx(
        "measurement,old,,boiler,,,,influxdb\n\
         tag,old,,boiler,site,,,influxdb\n\
         field,old,,boiler,temperature,float,,influxdb\n\
         field,old,,boiler,code,,,influxdb\n",
    );
    let b = influx(
        "measurement,new,,boiler,,,,influxdb\n\
         tag,new,,boiler,site,,,influxdb\n\
         field,new,,boiler,temperature,float,,influxdb\n\
         field,new,,boiler,code,string,,influxdb\n",
    );
    let m = match_schemas(&a, &b, &WordList::default(), &Overrides::default());
    let p = m.mapping("old.boiler").expect("matched by name");
    assert_eq!(p.target, "new.boiler");
    for r in &p.rows {
        let t = r.target.as_ref().expect("every column has a place");
        assert_eq!(t.name, r.source.name);
        assert_eq!(t.role, r.source.role);
    }
    let fit = |c: &str| {
        p.rows
            .iter()
            .find(|r| r.source.name == c)
            .unwrap()
            .compatibility
            .clone()
            .unwrap()
    };
    // Time into time keeps every digit; tags and fields keep their types.
    assert_eq!(fit("time").fit, Fit::Same);
    assert_eq!(fit("site").fit, Fit::Same);
    assert_eq!(fit("temperature").fit, Fit::Same);
    // An unknown type is not guessed.
    assert_eq!(fit("code").fit, Fit::Convert);
    assert!(fit("code").note.contains("not in the export"));
}

#[test]
fn a_tag_that_becomes_a_field_is_reported() {
    let a = influx(
        "measurement,b,,boiler,,,,influxdb\n\
         tag,b,,boiler,unit_id,,,influxdb\n\
         field,b,,boiler,temperature,float,,influxdb\n",
    );
    let b = influx(
        "measurement,b,,boiler,,,,influxdb\n\
         field,b,,boiler,unit_id,string,,influxdb\n\
         tag,b,,boiler,temperature,,,influxdb\n",
    );
    let f = analyse(&a, &b, "");
    let changes: Vec<&str> = f
        .iter()
        .filter(|f| f.rule == "MAP-TS-ROLE-CHANGE")
        .map(|f| f.title.as_str())
        .collect();
    assert_eq!(changes.len(), 2, "{f:?}");
}

#[test]
fn the_point_identity_is_reviewed_in_both_directions() {
    let plant = influx(
        "measurement,plant,,boiler,,,,influxdb\n\
         tag,plant,,boiler,site,,,influxdb\n\
         tag,plant,,boiler,unit_id,,,influxdb\n\
         field,plant,,boiler,temperature,float,,influxdb\n",
    );
    let words = "time = recorded_at";
    // InfluxDB → PostgreSQL: a finding only without a unique rule.
    assert!(rules(&analyse(&plant, &pg_boiler(false), words)).contains(&"MAP-TS-SERIES-KEY"));
    assert!(!rules(&analyse(&plant, &pg_boiler(true), words)).contains(&"MAP-TS-SERIES-KEY"));

    // PostgreSQL → InfluxDB: rows that share tags and time overwrite each
    // other unless the source keeps them unique.
    let back = "recorded_at = time";
    let f = analyse(&pg_boiler(false), &plant, back);
    let key = f
        .iter()
        .find(|f| f.rule == "MAP-TS-SERIES-KEY")
        .expect("reverse finding");
    assert_eq!(key.title, "Rows may overwrite each other");
    assert!(!rules(&analyse(&pg_boiler(true), &plant, back)).contains(&"MAP-TS-SERIES-KEY"));
}

#[test]
fn retention_differences_are_visible() {
    let bucket = |retention: &str| {
        influx(&format!(
            "bucket,b,{retention},,,,,influxdb\n\
             measurement,b,,cpu,,,,influxdb\n\
             tag,b,,cpu,host,,,influxdb\n\
             field,b,,cpu,usage,float,,influxdb\n"
        ))
    };
    let retention = |a: &Schema, b: &Schema| {
        analyse(a, b, "")
            .into_iter()
            .filter(|f| f.rule == "MAP-TS-RETENTION")
            .map(|f| f.severity)
            .collect::<Vec<_>>()
    };
    // Into a shorter retention: points are dropped.
    assert_eq!(retention(&bucket("90d"), &bucket("30d")), [Severity::High]);
    assert_eq!(retention(&bucket("INF"), &bucket("30d")), [Severity::High]);
    // Same or longer: nothing to report.
    assert!(retention(&bucket("30d"), &bucket("30d")).is_empty());
    assert!(retention(&bucket("30d"), &bucket("INF")).is_empty());
    // Into a relational table: the limited history is pointed out, and the
    // relational side is not pretended to have a retention period.
    let f = analyse(&bucket("30d"), &pg_boiler(false), "table: cpu = boiler");
    let r = f.iter().find(|f| f.rule == "MAP-TS-RETENTION").unwrap();
    assert_eq!(r.severity, Severity::Medium);
    assert!(r.explanation.contains("30 days") && r.explanation.contains("no retention period"));
    // Kept forever: nothing to say about history.
    assert!(
        !rules(&analyse(
            &bucket("INF"),
            &pg_boiler(false),
            "table: cpu = boiler"
        ))
        .contains(&"MAP-TS-RETENTION")
    );
}

#[test]
fn influxdb_1_and_2_exports_both_read() {
    // InfluxDB 1.x: database and retention policy, written db/rp.
    let v1 = influx(
        "bucket,telegraf/autogen,INF,,,,,influxdb\n\
         measurement,telegraf/autogen,,cpu,,,12,influxdb\n\
         tag,telegraf/autogen,,cpu,host,,,influxdb\n\
         field,telegraf/autogen,,cpu,usage_idle,float,,influxdb\n",
    );
    let cpu = v1.table("telegraf/autogen.cpu").expect("keyed by db/rp");
    assert_eq!(cpu.kind, TableKind::Measurement);
    assert_eq!(table_name(&cpu.key), "cpu");
    assert_eq!(
        retention_summary(&v1).as_deref(),
        Some("telegraf/autogen: kept forever")
    );
    // InfluxDB 2.x / 3.x: a bucket with a retention period.
    let v2 = influx(
        "bucket,metrics,720h,,,,,influxdb\n\
         measurement,metrics,,cpu,,,12,influxdb\n\
         tag,metrics,,cpu,host,,,influxdb\n\
         field,metrics,,cpu,usage_idle,float,,influxdb\n",
    );
    assert_eq!(retention_summary(&v2).as_deref(), Some("metrics: 30 days"));
    // Without bucket rows the retention is simply unknown.
    let bare = influx(
        "measurement,metrics,,cpu,,,,influxdb\n\
         field,metrics,,cpu,usage_idle,float,,influxdb\n",
    );
    assert_eq!(retention_summary(&bare), None);
    let roles: Vec<ColumnRole> = bare.tables[0].columns.iter().map(|c| c.role).collect();
    assert_eq!(roles, [ColumnRole::Timestamp, ColumnRole::Field]);
}

#[test]
fn no_sql_is_drafted_for_a_time_series_target() {
    let target = influx(
        "bucket,b,INF,,,,,influxdb\n\
         measurement,b,,boiler,,,,influxdb\n\
         tag,b,,boiler,site,,,influxdb\n\
         tag,b,,boiler,unit_id,,,influxdb\n\
         field,b,,boiler,temperature,float,,influxdb\n",
    );
    let source = pg_boiler(true);
    assert!(!draft_available(&target) && draft_available(&source));
    let m = match_schemas(
        &source,
        &target,
        &WordList::parse("recorded_at = time"),
        &Overrides::default(),
    );
    let sql = draft_sql(&source, &target, "S", "T", &m.mappings, &[]);
    assert!(sql.contains("No SQL is generated for T: InfluxDB is a time-series database."));
    assert!(sql.contains("line protocol"));
    assert!(!sql.contains("INSERT INTO") && !sql.contains("BEGIN;"));
    assert!(sql.lines().all(|l| l.starts_with("--")));
    let one = schema_bridge_core::sql::table_sql(&m.mappings[0], &source, &target, "S");
    assert!(
        one.contains("No SQL is generated for boiler: InfluxDB") && !one.contains("INSERT INTO")
    );

    // In the workspace the plan, report and summary say the same.
    let mut ws = Workspace::new();
    ws.add_file(Side::Source, "pg.csv", pg_csv());
    ws.add_file(Side::Target, "influx.csv", format!("{HEADER}measurement,b,,boiler,,,,influxdb\nfield,b,,boiler,temperature,float,,influxdb\n"));
    assert!(!ws.sql_available());
    let a = ws.analysis().unwrap();
    assert_eq!(summary(&ws, Some(&a)).todos, 0);
    let report = report_markdown(&ws, Some(&a));
    assert!(report.contains("Run a test load with the target's own tools"));
    assert!(report.contains("Not available: Target is a time-series database"));
    assert!(!report.contains("BEGIN;"));
}

fn pg_csv() -> String {
    format!(
        "{PG_HEADER}column,public,boiler,table,1,temperature,double precision,true,,,,,,,,,,,,,postgresql\n"
    )
}

#[test]
fn the_report_shows_measurements_tags_fields_and_retention() {
    let mut ws = Workspace::new();
    ws.load_timeseries_samples();
    let a = ws.analysis().unwrap();
    let s = summary(&ws, Some(&a));
    assert_eq!(s.source_noun, "measurements");
    assert_eq!(
        (s.tables.total, s.tables.matched, s.tables.no_place),
        (3, 2, 1)
    );
    let md = report_markdown(&ws, Some(&a));
    for expected in [
        "| Source measurements | 3 |",
        "| Time series | Plant (sample) | Readings (sample) |",
        "| Measurements | 3 | – |",
        "| Series | 622 | – |",
        "| Retention | plant: 30 days | – |",
        "nanosecond precision",
        "## Draft SQL",
        "CREATE TABLE staging.boiler",
    ] {
        assert!(md.contains(expected), "missing {expected:?}");
    }
}

#[test]
fn a_saved_time_series_comparison_opens_unchanged() {
    let mut ws = Workspace::new();
    ws.load_timeseries_samples();
    ws.set_table_choice(
        "plant.energy_meter",
        Some(schema_bridge_core::TableChoice::Stays),
    );
    let text = save_workspace(&ws);
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["version"], 2);
    assert_eq!(v["source"]["importer"], "influx-schema-csv");
    assert_eq!(v["target"]["importer"], "csv-export");
    let opened = open_workspace(&text).expect("opens");
    assert_eq!(opened.source.input, ws.source.input);
    assert_eq!(opened.source.schema, ws.source.schema);
    assert_eq!(opened.word_list, ws.word_list);
    assert_eq!(opened.overrides, ws.overrides);
    assert_eq!(opened.analysis(), ws.analysis());
    assert_eq!(save_workspace(&opened), text);
}

#[test]
fn the_sample_word_list_leaves_with_the_samples() {
    let mut ws = Workspace::new();
    ws.load_timeseries_samples();
    assert!(ws.word_list.contains("time = recorded_at"));
    ws.add_file(Side::Target, "mine.csv", pg_csv());
    assert!(!ws.using_samples);
    assert!(ws.word_list.is_empty());
}
