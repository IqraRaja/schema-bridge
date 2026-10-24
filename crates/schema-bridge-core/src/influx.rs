//! The Schema Bridge time-series schema export: a normalised CSV that
//! describes an InfluxDB bucket (its retention, measurements, tags, fields,
//! field types and series counts), and how it becomes the neutral model.
//!
//! This is Schema Bridge's own interchange format, not a native InfluxDB
//! format. It can be produced with the Flux script or the InfluxDB 3 SQL
//! query in `export-queries/`, or by any other tool; Flux "annotated CSV"
//! (comment rows starting with `#`, extra `result` and `table` columns,
//! repeated headers) is accepted unchanged. See docs/import-formats.md.

use std::collections::BTreeMap;

use crate::csv_reader::{self, get_nonempty};
use crate::import::ImportError;
use crate::model::{Column, ColumnRole, Provided, RetentionPolicy, Schema, Table, TableKind};
use crate::systems::normalise_type;

/// The columns of the export, in their usual order.
pub const COLUMNS: [&str; 8] = [
    "record_type",
    "bucket",
    "retention",
    "measurement",
    "key",
    "data_type",
    "series",
    "dbms",
];

/// Field types InfluxDB knows.
pub const FIELD_TYPES: [&str; 5] = ["float", "integer", "unsigned", "string", "boolean"];

/// How a type conflict is written in a field's type label: `float | integer`.
pub const CONFLICT_SEPARATOR: &str = " | ";

/// Marks a field that shares its key with a tag (see [`build`]).
pub const SAME_KEY_NOTE: &str = "shares its key with a tag";

/// Lines that carry data, with their line numbers (from 1): comment rows of
/// annotated CSV and blank lines are left out.
fn data_lines(text: &str) -> Vec<(usize, &str)> {
    text.strip_prefix('\u{feff}')
        .unwrap_or(text)
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.trim_end_matches('\r')))
        .filter(|(_, l)| !l.trim().is_empty() && !l.starts_with('#'))
        .collect()
}

fn header_of(line: &str) -> Vec<String> {
    csv_reader::parse(line).header
}

/// Whether a file is a time-series schema export: its header has
/// `record_type`, `measurement` and `key`, and no relational `table_name`.
pub fn recognises(text: &str) -> bool {
    let Some((_, first)) = data_lines(text).into_iter().next() else {
        return false;
    };
    let header = header_of(first);
    let has = |name: &str| header.iter().any(|h| h == name);
    has("record_type") && has("measurement") && has("key") && !has("table_name")
}

/// Retention as InfluxDB writes it: `30d`, `720h0m0s`, `1w2d`, `90m`, or
/// `INF`/`0` for "keep forever". `Some(None)` is infinite, `Some(Some(s))`
/// is a number of seconds, `None` is not a duration.
pub fn parse_retention(text: &str) -> Option<Option<u64>> {
    let t = text.trim().to_lowercase();
    if matches!(t.as_str(), "inf" | "infinite" | "0" | "0s") {
        return Some(None);
    }
    let mut total: u64 = 0;
    let mut number = String::new();
    let mut seen_unit = false;
    for c in t.chars() {
        if c.is_ascii_digit() {
            number.push(c);
            continue;
        }
        let n: u64 = number.parse().ok()?;
        number.clear();
        let unit = match c {
            'w' => 7 * 24 * 3600,
            'd' => 24 * 3600,
            'h' => 3600,
            'm' => 60,
            's' => 1,
            _ => return None,
        };
        total = total.checked_add(n.checked_mul(unit)?)?;
        seen_unit = true;
    }
    if !number.is_empty() || !seen_unit {
        return None;
    }
    Some(if total == 0 { None } else { Some(total) })
}

/// A retention period in words: "30 days", "12 hours", "kept forever".
pub fn describe_retention(seconds: Option<u64>) -> String {
    let plural = |n: u64, unit: &str| format!("{n} {unit}{}", if n == 1 { "" } else { "s" });
    match seconds {
        None => "kept forever".to_string(),
        Some(s) if s % 86_400 == 0 => plural(s / 86_400, "day"),
        Some(s) if s % 3_600 == 0 => plural(s / 3_600, "hour"),
        Some(s) if s % 60 == 0 => plural(s / 60, "minute"),
        Some(s) => plural(s, "second"),
    }
}

/// The retention of a measurement's bucket, in seconds (`Some(None)` for
/// infinite), or `None` when the export did not say.
pub fn retention_of(schema: &Schema, table: &Table) -> Option<Option<u64>> {
    let name = table.retention_policy.as_deref()?;
    let policy = schema.retention_policies.iter().find(|p| p.name == name)?;
    parse_retention(policy.duration.as_deref()?)
}

/// Each bucket with how long it keeps data, for example
/// `plant: 30 days`; `None` when the export says nothing about retention.
pub fn retention_summary(schema: &Schema) -> Option<String> {
    let parts: Vec<String> = schema
        .retention_policies
        .iter()
        .map(|p| {
            let kept = p
                .duration
                .as_deref()
                .and_then(parse_retention)
                .map_or("not stated".to_string(), describe_retention);
            format!("{}: {kept}", p.name)
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

struct Measurement {
    bucket: String,
    name: String,
    series: Option<i64>,
    line: usize,
    tags: Vec<String>,
    /// Field key and the types seen for it, in order.
    fields: Vec<(String, Vec<String>)>,
}

fn invalid(line: usize, reason: impl Into<String>) -> ImportError {
    ImportError::Invalid {
        line,
        reason: reason.into(),
    }
}

/// Reads an export into the neutral model. Measurements become tables of
/// kind `Measurement` with an explicit `time` column, tag columns and field
/// columns; buckets become retention policies. Every problem is reported
/// with its line number.
pub fn build(text: &str) -> Result<Schema, ImportError> {
    let lines = data_lines(text);
    let Some(&(_, header_line)) = lines.first() else {
        return Err(ImportError::Unrecognised);
    };
    let header = header_line.to_string();
    let mut buckets: Vec<(String, Option<String>)> = Vec::new();
    let mut measurements: Vec<Measurement> = Vec::new();
    let mut members: Vec<(usize, &'static str, String, String, String, String)> = Vec::new();
    let mut dbms: Option<String> = None;

    for &(line, raw) in &lines[1..] {
        let parsed = csv_reader::parse(&format!("{header}\n{raw}"));
        let Some(row) = parsed.rows.into_iter().next() else {
            continue;
        };
        let value = |name: &str| get_nonempty(&row, name).map(|v| v.trim().to_string());
        let record_type = value("record_type").unwrap_or_default().to_lowercase();
        if record_type == "record_type" {
            continue; // a header repeated by annotated CSV
        }
        if let Some(d) = value("dbms") {
            dbms.get_or_insert(d.to_lowercase());
        }
        let bucket = value("bucket");
        match record_type.as_str() {
            "bucket" => {
                let name =
                    bucket.ok_or_else(|| invalid(line, "A bucket row needs a bucket name."))?;
                let retention = value("retention");
                if let Some(r) = &retention {
                    if parse_retention(r).is_none() {
                        return Err(invalid(
                            line,
                            format!(
                                "Retention \"{r}\" is not a duration (for example 30d, 720h or INF)."
                            ),
                        ));
                    }
                }
                if !buckets.iter().any(|(b, _)| *b == name) {
                    buckets.push((name, retention));
                }
            }
            "measurement" => {
                let bucket =
                    bucket.ok_or_else(|| invalid(line, "A measurement row needs its bucket."))?;
                let name = value("measurement").ok_or_else(|| {
                    invalid(line, "A measurement row needs the measurement name.")
                })?;
                let series = match value("series") {
                    None => None,
                    Some(s) => {
                        Some(s.parse::<i64>().ok().filter(|n| *n >= 0).ok_or_else(|| {
                            invalid(line, format!("Series count \"{s}\" is not a whole number."))
                        })?)
                    }
                };
                if !measurements
                    .iter()
                    .any(|m| m.bucket == bucket && m.name == name)
                {
                    measurements.push(Measurement {
                        bucket,
                        name,
                        series,
                        line,
                        tags: Vec::new(),
                        fields: Vec::new(),
                    });
                }
            }
            "tag" | "field" => {
                let kind = if record_type == "tag" { "tag" } else { "field" };
                let bucket = bucket
                    .ok_or_else(|| invalid(line, format!("A {kind} row needs its bucket.")))?;
                let measurement = value("measurement")
                    .ok_or_else(|| invalid(line, format!("A {kind} row needs its measurement.")))?;
                let key = value("key")
                    .ok_or_else(|| invalid(line, format!("A {kind} row needs its key.")))?;
                let data_type = value("data_type").unwrap_or_default().to_lowercase();
                if kind == "field"
                    && !data_type.is_empty()
                    && !FIELD_TYPES.contains(&data_type.as_str())
                {
                    return Err(invalid(
                        line,
                        format!(
                            "Unknown field type \"{data_type}\": expected float, integer, unsigned, string or boolean, or nothing when the type is not known."
                        ),
                    ));
                }
                members.push((line, kind, bucket, measurement, key, data_type));
            }
            other => {
                return Err(invalid(
                    line,
                    format!(
                        "Unknown record type \"{other}\": expected bucket, measurement, tag or field."
                    ),
                ));
            }
        }
    }

    for (line, kind, bucket, measurement, key, data_type) in members {
        let Some(m) = measurements
            .iter_mut()
            .find(|m| m.bucket == bucket && m.name == measurement)
        else {
            return Err(invalid(
                line,
                format!(
                    "The {kind} \"{key}\" belongs to measurement \"{measurement}\", which has no measurement row."
                ),
            ));
        };
        if kind == "tag" {
            if !m.tags.contains(&key) {
                m.tags.push(key);
            }
        } else {
            match m.fields.iter_mut().find(|(k, _)| *k == key) {
                Some((_, types)) => {
                    if !data_type.is_empty() && !types.contains(&data_type) {
                        types.push(data_type);
                    }
                }
                None => m.fields.push((
                    key,
                    if data_type.is_empty() {
                        vec![]
                    } else {
                        vec![data_type]
                    },
                )),
            }
        }
    }
    if measurements.is_empty() {
        return Err(invalid(
            lines.len().max(1),
            "The export contains no measurements.",
        ));
    }
    if !buckets.is_empty() {
        if let Some(m) = measurements
            .iter()
            .find(|m| !buckets.iter().any(|(b, _)| *b == m.bucket))
        {
            return Err(invalid(
                m.line,
                format!(
                    "Measurement \"{}\" belongs to bucket \"{}\", which has no bucket row.",
                    m.name, m.bucket
                ),
            ));
        }
    }

    let dbms = dbms.unwrap_or_else(|| "influxdb".to_string());
    let column =
        |name: &str, label: &str, role: ColumnRole, position: i64, nullable: bool| Column {
            name: name.to_string(),
            type_label: label.to_string(),
            data_type: normalise_type(label, &dbms),
            nullable,
            position,
            role,
            ..Default::default()
        };
    let mut tables: Vec<Table> = measurements
        .into_iter()
        .map(|m| {
            let mut columns = vec![column(
                "time",
                "timestamp (ns)",
                ColumnRole::Timestamp,
                1,
                false,
            )];
            for tag in &m.tags {
                let position = columns.len() as i64 + 1;
                columns.push(column(tag, "string", ColumnRole::Tag, position, true));
            }
            for (key, types) in &m.fields {
                let position = columns.len() as i64 + 1;
                // InfluxQL returns a field that shares a tag's key as `key_1`.
                let same_key = m.tags.contains(key);
                let name = if same_key {
                    format!("{key}_1")
                } else {
                    key.clone()
                };
                let mut c = match types.as_slice() {
                    [] => Column {
                        data_type: String::new(),
                        ..column(&name, "", ColumnRole::Field, position, true)
                    },
                    [one] => column(&name, one, ColumnRole::Field, position, true),
                    many => Column {
                        data_type: String::new(),
                        ..column(
                            &name,
                            &many.join(CONFLICT_SEPARATOR),
                            ColumnRole::Field,
                            position,
                            true,
                        )
                    },
                };
                if same_key {
                    c.comment = Some(format!(
                        "Field \"{key}\" {SAME_KEY_NOTE}; queries return it as {name}."
                    ));
                }
                columns.push(c);
            }
            Table {
                key: format!("{}.{}", m.bucket, m.name),
                schema: m.bucket.clone(),
                name: m.name,
                columns,
                kind: TableKind::Measurement,
                retention_policy: (!buckets.is_empty()).then_some(m.bucket),
                series: m.series,
                ..Default::default()
            }
        })
        .collect();
    tables.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.schema.cmp(&b.schema))
    });
    let retention_policies = buckets
        .into_iter()
        .map(|(name, duration)| RetentionPolicy {
            name,
            duration,
            default: true,
        })
        .collect();
    Ok(Schema {
        dbms,
        tables,
        retention_policies,
        has: Provided {
            columns: true,
            constraints: false,
            indexes: false,
            row_counts: false,
        },
        ..Default::default()
    })
}

/// Field keys with the types seen for them, for tests and the importer.
pub fn field_types(table: &Table) -> BTreeMap<&str, &str> {
    table
        .columns
        .iter()
        .filter(|c| c.role == ColumnRole::Field)
        .map(|c| (c.name.as_str(), c.type_label.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "record_type,bucket,retention,measurement,key,data_type,series,dbms";

    fn export(rows: &[&str]) -> String {
        let mut s = String::from(HEADER);
        for r in rows {
            s.push('\n');
            s.push_str(r);
        }
        s.push('\n');
        s
    }

    fn sample() -> String {
        std::fs::read_to_string(format!(
            "{}/../../demo/plant-influxdb.csv",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    #[test]
    fn recognises_only_time_series_exports() {
        assert!(recognises(&export(&[])));
        assert!(recognises(&format!(
            "#datatype,string,long\n,result,table,{HEADER}\n"
        )));
        let relational = std::fs::read_to_string(format!(
            "{}/../../demo/workshop-sqlite.csv",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        assert!(!recognises(&relational));
        assert!(!recognises("item,price\nbread,2\n"));
        assert!(!recognises(""));
        assert!(!recognises("record_type,table_name,measurement,key\n"));
    }

    #[test]
    fn reads_buckets_measurements_tags_fields_and_series() {
        let s = build(&export(&[
            "bucket,plant,30d,,,,,influxdb",
            "measurement,plant,,boiler,,,412,influxdb",
            "tag,plant,,boiler,site,,,influxdb",
            "tag,plant,,boiler,unit_id,,,influxdb",
            "field,plant,,boiler,temperature,float,,influxdb",
            "field,plant,,boiler,running,boolean,,influxdb",
        ]))
        .unwrap();
        assert_eq!(s.dbms, "influxdb");
        assert!(s.is_time_series());
        assert_eq!(s.retention_policies.len(), 1);
        assert_eq!(s.retention_policies[0].duration.as_deref(), Some("30d"));
        let t = &s.tables[0];
        assert_eq!(
            (t.key.as_str(), t.schema.as_str(), t.name.as_str()),
            ("plant.boiler", "plant", "boiler")
        );
        assert_eq!(t.kind, TableKind::Measurement);
        assert_eq!(t.retention_policy.as_deref(), Some("plant"));
        assert_eq!(t.series, Some(412));
        assert!(t.rows.is_none() && t.primary_key.is_empty() && t.foreign_keys.is_empty());
        let roles: Vec<(&str, ColumnRole, &str, bool)> = t
            .columns
            .iter()
            .map(|c| (c.name.as_str(), c.role, c.data_type.as_str(), c.nullable))
            .collect();
        assert_eq!(
            roles,
            [
                (
                    "time",
                    ColumnRole::Timestamp,
                    "timestamp with time zone",
                    false
                ),
                ("site", ColumnRole::Tag, "text", true),
                ("unit_id", ColumnRole::Tag, "text", true),
                ("temperature", ColumnRole::Field, "double precision", true),
                ("running", ColumnRole::Field, "boolean", true),
            ]
        );
        assert_eq!(t.columns[0].type_label, "timestamp (ns)");
        assert_eq!(retention_of(&s, t), Some(Some(30 * 86_400)));
        assert!(!s.has.constraints && !s.has.indexes && !s.has.row_counts && s.has.columns);
    }

    #[test]
    fn every_field_type_is_normalised_and_unknown_types_stay_unknown() {
        let s = build(&export(&[
            "measurement,b,,m,,,,",
            "field,b,,m,f,float,,",
            "field,b,,m,i,integer,,",
            "field,b,,m,u,unsigned,,",
            "field,b,,m,s,string,,",
            "field,b,,m,t,boolean,,",
            "field,b,,m,x,,,",
        ]))
        .unwrap();
        let types: Vec<(&str, &str, &str)> = s.tables[0].columns[1..]
            .iter()
            .map(|c| (c.name.as_str(), c.type_label.as_str(), c.data_type.as_str()))
            .collect();
        assert_eq!(
            types,
            [
                ("f", "float", "double precision"),
                ("i", "integer", "bigint"),
                ("u", "unsigned", "bigint"),
                ("s", "string", "text"),
                ("t", "boolean", "boolean"),
                ("x", "", ""),
            ]
        );
        // Without a dbms column the system is InfluxDB; without bucket rows
        // the retention is unknown.
        assert_eq!(s.dbms, "influxdb");
        assert!(s.retention_policies.is_empty() && s.tables[0].retention_policy.is_none());
    }

    #[test]
    fn type_conflicts_and_shared_keys_are_kept_visible() {
        let s = build(&export(&[
            "measurement,b,,m,,,,",
            "tag,b,,m,host,,,",
            "field,b,,m,speed,integer,,",
            "field,b,,m,speed,float,,",
            "field,b,,m,speed,integer,,",
            "field,b,,m,host,string,,",
        ]))
        .unwrap();
        let t = &s.tables[0];
        let speed = t.column("speed").unwrap();
        assert_eq!(speed.type_label, "integer | float");
        assert_eq!(speed.data_type, "", "a conflict compares as unknown");
        let host = t.column("host_1").unwrap();
        assert_eq!(host.role, ColumnRole::Field);
        assert!(host.comment.as_deref().unwrap().contains(SAME_KEY_NOTE));
        assert_eq!(t.column("host").unwrap().role, ColumnRole::Tag);
    }

    #[test]
    fn flux_annotated_csv_is_accepted_unchanged() {
        let text = format!(
            "#group,false,false,false,false,false,false,false,false,false,false\n\
             #datatype,string,long,string,string,string,string,string,string,string,string\n\
             #default,_result,,,,,,,,,\n\
             ,result,table,{HEADER}\n\
             ,,0,bucket,plant,720h0m0s,,,,,influxdb\n\
             ,,0,measurement,plant,,boiler,,,,influxdb\n\
             \n\
             ,result,table,{HEADER}\n\
             ,,1,tag,plant,,boiler,site,,,influxdb\n\
             ,,1,field,plant,,boiler,temperature,float,,influxdb\n"
        );
        assert!(recognises(&text));
        let s = build(&text).unwrap();
        assert_eq!(retention_of(&s, &s.tables[0]), Some(Some(30 * 86_400)));
        assert_eq!(s.tables[0].columns.len(), 3);
    }

    #[test]
    fn malformed_lines_are_refused_with_their_line_number() {
        let error = |rows: &[&str]| build(&export(rows)).unwrap_err().to_string();
        assert_eq!(
            error(&["measurement,b,,m,,,,", "column,b,,m,x,,,"]),
            "Line 3: Unknown record type \"column\": expected bucket, measurement, tag or field."
        );
        assert!(
            error(&["measurement,b,,m,,,,", "field,b,,m,x,double,,"])
                .starts_with("Line 3: Unknown field type \"double\"")
        );
        assert!(error(&["bucket,b,soon,,,,,"]).starts_with("Line 2: Retention \"soon\""));
        assert!(error(&["measurement,b,,m,,,many,"]).starts_with("Line 2: Series count \"many\""));
        assert!(error(&["measurement,b,,,,,,"]).contains("needs the measurement name"));
        assert!(error(&["measurement,b,,m,,,,", "tag,b,,m,,,,"]).contains("needs its key"));
        assert!(error(&["tag,b,,ghost,site,,,"]).contains("which has no measurement row"));
        assert!(
            error(&["bucket,a,30d,,,,,", "measurement,b,,m,,,,"])
                .contains("which has no bucket row")
        );
        assert!(error(&[]).contains("no measurements"));
    }

    #[test]
    fn retention_periods_are_read_in_every_usual_spelling() {
        assert_eq!(parse_retention("30d"), Some(Some(2_592_000)));
        assert_eq!(parse_retention("720h0m0s"), Some(Some(2_592_000)));
        assert_eq!(parse_retention("1w2d"), Some(Some(9 * 86_400)));
        assert_eq!(parse_retention("90m"), Some(Some(5_400)));
        for forever in ["INF", "inf", "infinite", "0", "0s"] {
            assert_eq!(parse_retention(forever), Some(None), "{forever}");
        }
        for bad in ["", "30", "d", "30x", "soon"] {
            assert_eq!(parse_retention(bad), None, "{bad}");
        }
        assert_eq!(describe_retention(Some(2_592_000)), "30 days");
        assert_eq!(describe_retention(Some(3_600)), "1 hour");
        assert_eq!(describe_retention(None), "kept forever");
    }

    #[test]
    fn the_sample_bucket_reads_completely() {
        let s = build(&sample()).unwrap();
        let names: Vec<&str> = s.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["boiler", "energy_meter", "pump"]);
        let boiler = &s.tables[0];
        assert_eq!(boiler.series, Some(412));
        assert_eq!(field_types(boiler).get("firmware_version"), Some(&""));
        assert_eq!(
            field_types(&s.tables[2]).get("speed_rpm"),
            Some(&"integer | float")
        );
    }
}
