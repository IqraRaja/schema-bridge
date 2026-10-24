//! Parity tests: type recognition and compatibility must give the same
//! answers as the prototype's `tinfo` and `compat`.
//!
//! `tests/fixtures/*.input.txt` list the types and type pairs (from all five
//! supported systems); `*.expected.txt` hold the prototype's answers.

use std::fs;
use std::path::PathBuf;

use schema_bridge_core::{Category, Fit, UNLIMITED, compatibility, type_info};

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn category(c: Category) -> &'static str {
    match c {
        Category::Bool => "bool",
        Category::Text => "text",
        Category::Int => "int",
        Category::Float => "float",
        Category::Numeric => "numeric",
        Category::Timestamp => "ts",
        Category::Date => "date",
        Category::Time => "time",
        Category::Uuid => "uuid",
        Category::Json => "json",
        Category::Binary => "bytea",
        Category::Xml => "xml",
        Category::Interval => "interval",
        Category::Other => "other",
    }
}

fn fit(f: Fit) -> &'static str {
    match f {
        Fit::Same => "same",
        Fit::Safe => "safe",
        Fit::Convert => "convert",
        Fit::Loss => "loss",
        Fit::Incompatible => "incompatible",
    }
}

fn assert_lines_equal(actual: &str, expected: &str) {
    for (a, e) in actual.lines().zip(expected.lines()) {
        assert_eq!(a, e);
    }
    assert_eq!(actual.lines().count(), expected.lines().count());
}

#[test]
fn type_recognition_matches_prototype() {
    let actual: String = fixture("types.input.txt")
        .lines()
        .filter(|l| !l.is_empty())
        .map(|name| {
            let t = type_info(name);
            let opt = |v: Option<String>| v.unwrap_or_else(|| "null".into());
            let size = if t.size == UNLIMITED {
                "unlimited".to_string()
            } else {
                t.size.to_string()
            };
            format!(
                "{name} => raw={} base={} cat={} size={size} len={} tz={} array={}\n",
                t.raw,
                t.base,
                category(t.category),
                opt(t.length.map(|l| l.to_string())),
                opt(t.time_zone.map(|z| z.to_string())),
                t.array
            )
        })
        .collect();
    assert_lines_equal(&actual, &fixture("types.expected.txt"));
}

#[test]
fn type_compatibility_matches_prototype() {
    let actual: String = fixture("compat.input.txt")
        .lines()
        .filter(|l| !l.is_empty())
        .map(|pair| {
            let (a, b) = pair.split_once('|').expect("pair is written a|b");
            let c = compatibility(a, b);
            format!(
                "{a} | {b} => {} lookup={} : {}\n",
                fit(c.fit),
                c.lookup,
                c.note
            )
        })
        .collect();
    assert_lines_equal(&actual, &fixture("compat.expected.txt"));
}
