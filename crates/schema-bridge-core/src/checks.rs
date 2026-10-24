//! Problem detection inside one database: a port of the prototype's
//! `dbChecks`. Every rule works on the schema model alone, so no data and no
//! database connection are needed.
//!
//! Each [`Finding`] says how serious the problem is, which object it is about,
//! what it means in plain words and what to do about it. The order of the
//! findings is the prototype's: table by table, then the checks that look at
//! the whole database.

use crate::model::{Column, ColumnRole, Schema, Table, TableKind, table_name};
use crate::types::{Category, type_info};

/// How serious a finding is. Ordered from most to least serious.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Must be solved before anything can be loaded.
    Blocking,
    High,
    Medium,
    Low,
    /// Good to know; nothing is wrong.
    Info,
}

impl Severity {
    pub const ALL: [Severity; 5] = [
        Severity::Blocking,
        Severity::High,
        Severity::Medium,
        Severity::Low,
        Severity::Info,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Severity::Blocking => "Blocking",
            Severity::High => "High",
            Severity::Medium => "Medium",
            Severity::Low => "Low",
            Severity::Info => "Info",
        }
    }
}

/// One problem found in a database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Rule id from the specification's rule catalogue, for example
    /// `KEY-MISSING-PK`.
    pub rule: &'static str,
    pub severity: Severity,
    /// The kind of problem, for example "Keys", "Links" or "Speed".
    pub area: String,
    /// The table, column or link the finding is about.
    pub object: String,
    pub title: String,
    pub explanation: String,
    /// What to do about it.
    pub fix: String,
}

/// Every rule id a finding can carry: the specification's rule catalogue
/// (section 7) plus two prototype checks the catalogue does not list
/// (`NAME-LINK-COLUMN`, `KEY-MIXED-GENERATORS`).
pub const RULES: &[&str] = &[
    "KEY-MISSING-PK",
    "LINK-WRONG-TARGET",
    "LINK-NOT-VALIDATED",
    "LINK-IMPLICIT",
    "LINK-SHARED-CODE",
    "LINK-ID-ARRAY",
    "DUP-IDENTIFIER",
    "DUP-NULLABLE-UNIQUE",
    "KEY-SURROGATE-NOT-UNIQUE",
    "TIME-MIXED-TZ",
    "DATA-MULTI-FORMAT",
    "SEC-SENSITIVE",
    "PERF-FK-NO-INDEX",
    "PERF-DUP-INDEX",
    "NAME-RESERVED",
    "NAME-MISLEADING",
    "DOC-COVERAGE",
    "CONTENT-EMPTY",
    "NAME-LINK-COLUMN",
    "KEY-MIXED-GENERATORS",
];

/// Rule ids of the time-series checks (docs/time-series.md).
pub const TIME_SERIES_RULES: &[&str] = &[
    "TS-MEASUREMENT-NO-FIELDS",
    "TS-FIELD-TYPE-CONFLICT",
    "TS-FIELD-TYPE-UNKNOWN",
    "TS-UNSIGNED-FIELD",
    "TS-TAG-FIELD-SAME-KEY",
    "TS-HIGH-CARDINALITY",
];

/// Words that must be quoted when used as names in SQL (the prototype's list).
const RESERVED: &[&str] = &[
    "user",
    "end",
    "order",
    "group",
    "table",
    "limit",
    "offset",
    "default",
    "check",
    "column",
    "select",
    "from",
    "where",
    "to",
    "primary",
    "references",
    "analyse",
    "analyze",
    "all",
    "and",
    "any",
    "as",
    "both",
    "case",
    "cast",
    "constraint",
    "create",
    "current_date",
    "current_time",
    "current_user",
    "desc",
    "distinct",
    "do",
    "else",
    "except",
    "false",
    "fetch",
    "for",
    "foreign",
    "grant",
    "having",
    "in",
    "into",
    "join",
    "leading",
    "not",
    "null",
    "on",
    "only",
    "or",
    "returning",
    "session_user",
    "some",
    "symmetric",
    "then",
    "trailing",
    "true",
    "union",
    "unique",
    "using",
    "when",
    "window",
    "with",
];

/// Column names that should identify one record.
const IDENTIFIERS: &[&str] = &[
    "doi",
    "orcid",
    "email",
    "e_mail",
    "isbn",
    "issn",
    "ror",
    "shorthand_id",
    "short_id",
    "serial",
    "serial_number",
    "serial_no",
    "account_name",
    "inventory_number",
    "device_id",
];

/// Name endings that suggest a password or secret.
const SECRETS: &[&str] = &[
    "pass",
    "password",
    "passwd",
    "pwd",
    "secret",
    "token",
    "apikey",
    "api_key",
    "private_key",
];

/// Name endings of number columns that usually hold a code.
const CODE_ENDINGS: &[&str] = &["_nr", "_no", "_code", "_typ", "_type"];

/// Name endings of columns that hold the same data in a particular format.
const FORMAT_ENDINGS: &[&str] = &[
    "xml", "json", "jsonb", "bytea", "bin", "binary", "blob", "str", "string",
];

pub fn is_reserved(name: &str) -> bool {
    RESERVED.contains(&name)
}

/// Quotes a PostgreSQL name when it needs quotes.
pub(crate) fn quote(name: &str) -> String {
    let mut chars = name.chars();
    let plain = chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if plain && !is_reserved(name) {
        name.to_string()
    } else {
        format!("\"{}\"", name.replace('"', "\"\""))
    }
}

pub(crate) fn has_text(value: &Option<String>) -> bool {
    value.as_deref().is_some_and(|v| !v.is_empty())
}

/// Whether a single column is unique on its own (primary key, unique rule or
/// unique index that is not partial).
pub(crate) fn is_unique_column(table: &Table, column: &str) -> bool {
    (table.primary_key.len() == 1 && table.primary_key[0] == column)
        || table
            .uniques
            .iter()
            .any(|u| u.columns.len() == 1 && u.columns[0] == column)
        || table
            .indexes
            .iter()
            .any(|i| i.unique && !i.partial && i.columns.len() == 1 && i.columns[0] == column)
}

/// The words of a link column's name without `uuid_`/`fk_` at the start and
/// `_id`/`_uuid`/`_fk` at the end: `parent_asset_id` → `["parent", "asset"]`.
fn link_name_parts(column: &str) -> Vec<String> {
    let mut name = column.to_lowercase();
    for prefix in ["uuid_", "fk_"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            name = rest.to_string();
            break;
        }
    }
    for suffix in ["_id", "_uuid", "_fk"] {
        if let Some(rest) = name.strip_suffix(suffix) {
            name = rest.to_string();
            break;
        }
    }
    name.split('_').map(str::to_string).collect()
}

/// `password`, `user_token`, …, but not `tokens` or `bypass`.
fn looks_secret(name: &str) -> bool {
    SECRETS.iter().any(|s| {
        name.strip_suffix(s)
            .is_some_and(|before| before.is_empty() || before.ends_with('_'))
    })
}

/// A default such as `gen_random_uuid()` without its arguments, as the
/// prototype shows it. (Its pattern stops at a line break.)
fn generator_name(default: &str) -> String {
    for (i, c) in default.char_indices() {
        if c == '(' && !default[i..].contains('\n') {
            return default[..i].to_string();
        }
    }
    default.to_string()
}

/// Groups values under a key, keeping the order in which keys first appear.
fn push_group<V>(groups: &mut Vec<(String, Vec<V>)>, key: &str, value: V) {
    match groups.iter_mut().find(|(k, _)| k == key) {
        Some((_, values)) => values.push(value),
        None => groups.push((key.to_string(), vec![value])),
    }
}

/// Checks one database for structural problems. Relational rules look only
/// at tables, time-series rules (`TS-*`) only at measurements, so a
/// relational schema gets exactly the findings it always got.
pub fn check_schema(schema: &Schema) -> Vec<Finding> {
    if !schema.is_time_series() {
        return check_relational(schema);
    }
    let relational = Schema {
        tables: schema
            .tables
            .iter()
            .filter(|t| t.kind == TableKind::Table)
            .cloned()
            .collect(),
        ..schema.clone()
    };
    let mut out = check_relational(&relational);
    out.extend(check_time_series(schema));
    out
}

/// Tag names whose values usually grow without bound.
const UNBOUNDED_TAGS: &[&str] = &["uuid", "request_id", "session_id", "trace_id", "span_id"];

/// Series counts from which a measurement is reported as high-cardinality.
pub const HIGH_CARDINALITY: i64 = 100_000;

/// The time-series rules, measurement by measurement.
fn check_time_series(schema: &Schema) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut add = |rule: &'static str,
                   severity,
                   object: String,
                   title: &str,
                   explanation: String,
                   fix: &str| {
        out.push(Finding {
            rule,
            severity,
            area: "Time series".into(),
            object,
            title: title.into(),
            explanation,
            fix: fix.into(),
        })
    };
    for t in schema
        .tables
        .iter()
        .filter(|t| t.kind == TableKind::Measurement)
    {
        let fields: Vec<&Column> = t
            .columns
            .iter()
            .filter(|c| c.role == ColumnRole::Field)
            .collect();
        if fields.is_empty() {
            add(
                "TS-MEASUREMENT-NO-FIELDS",
                Severity::High,
                t.name.clone(),
                "Measurement has no fields",
                "InfluxDB stores values only in fields, so a measurement without fields holds no data to migrate. The export may be incomplete.".into(),
                "Check the export, or decide that this measurement is not migrated.",
            );
        }
        for c in &fields {
            let object = format!("{}.{}", t.name, c.name);
            if c.type_label.contains(crate::influx::CONFLICT_SEPARATOR) {
                add(
                    "TS-FIELD-TYPE-CONFLICT",
                    Severity::High,
                    object.clone(),
                    "Field has more than one type",
                    format!(
                        "InfluxDB keeps a field's type per shard, so {} holds values of these types: {}. A typed target column accepts only one of them.",
                        c.name, c.type_label
                    ),
                    "Decide one type (usually the wider one, such as float) and convert the other values when loading.",
                );
            } else if c.type_label.is_empty() {
                add(
                    "TS-FIELD-TYPE-UNKNOWN",
                    Severity::Medium,
                    object.clone(),
                    "Field type unknown",
                    format!(
                        "The export does not say which type {} has (for example because it was made with Flux schema functions, which report names only). Schema Bridge cannot judge whether it fits its target column.",
                        c.name
                    ),
                    "Export the field types (InfluxQL SHOW FIELD KEYS, or the Flux script in export-queries/), or check the values before loading.",
                );
            } else if c.type_label == "unsigned" {
                add(
                    "TS-UNSIGNED-FIELD",
                    Severity::Info,
                    object.clone(),
                    "Unsigned integer field",
                    format!(
                        "{} is an unsigned 64-bit integer. Schema Bridge compares it as a signed 64-bit integer (bigint); values above 9,223,372,036,854,775,807 would not fit.",
                        c.name
                    ),
                    "Check the largest value; use numeric(20,0) in the target if values can exceed the signed range.",
                );
            }
            if c.comment
                .as_deref()
                .is_some_and(|n| n.contains(crate::influx::SAME_KEY_NOTE))
            {
                add(
                    "TS-TAG-FIELD-SAME-KEY",
                    Severity::Medium,
                    object,
                    "A tag and a field share a key",
                    format!(
                        "{} uses the same key for a tag and a field. Queries return the field as {}, and a target table needs two differently named columns.",
                        t.name, c.name
                    ),
                    "Rename one of them in the target, or keep only one if they hold the same information.",
                );
            }
        }
        if let Some(series) = t.series.filter(|s| *s >= HIGH_CARDINALITY) {
            add(
                "TS-HIGH-CARDINALITY",
                Severity::Medium,
                t.name.clone(),
                "Many series",
                format!(
                    "{} has {} series (tag combinations). High cardinality slows InfluxDB and becomes a large, heavily indexed table in a relational target.",
                    t.name,
                    crate::workspace::grouped(series)
                ),
                "Check whether every tag is needed to identify a point; values such as IDs are usually better stored as fields.",
            );
        }
        for c in t.columns.iter().filter(|c| c.role == ColumnRole::Tag) {
            if UNBOUNDED_TAGS.iter().any(|u| c.name.to_lowercase() == *u) {
                add(
                    "TS-HIGH-CARDINALITY",
                    Severity::Low,
                    format!("{}.{}", t.name, c.name),
                    "Tag may have unbounded values",
                    format!(
                        "Every new value of the tag {} creates a new series, and values like this one usually never repeat.",
                        c.name
                    ),
                    "Consider storing it as a field instead of a tag.",
                );
            }
        }
    }
    out
}

/// The relational rules (the prototype's `dbChecks`).
fn check_relational(schema: &Schema) -> Vec<Finding> {
    let mut out = Vec::new();
    if schema.tables.is_empty() {
        return out;
    }
    let mut add = |rule: &'static str,
                   severity,
                   area: &str,
                   object: String,
                   title: String,
                   explanation: String,
                   fix: String| {
        out.push(Finding {
            rule,
            severity,
            area: area.to_string(),
            object,
            title,
            explanation,
            fix,
        })
    };

    let by_name: Vec<(&Table, Vec<String>)> = schema
        .tables
        .iter()
        .map(|t| {
            let parts = t
                .name
                .to_lowercase()
                .split('_')
                .map(str::to_string)
                .collect();
            (t, parts)
        })
        .collect();
    let mut code_groups: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let mut reserved_hits = Vec::new();
    let mut empty_tables = Vec::new();
    let mut unanalysed = Vec::new();
    let mut bad_names = Vec::new();
    let mut generators: Vec<String> = Vec::new();
    let (mut columns, mut columns_documented, mut tables_documented) = (0, 0, 0);

    for t in &schema.tables {
        columns += t.columns.len();
        columns_documented += t.columns.iter().filter(|c| has_text(&c.comment)).count();
        if has_text(&t.comment) {
            tables_documented += 1;
        }
        let name = t.name.as_str();

        if schema.has.constraints && t.primary_key.is_empty() {
            add(
                "KEY-MISSING-PK",
                Severity::High,
                "Keys",
                name.to_string(),
                "Table has no primary key".into(),
                "Rows have no guaranteed ID, so a row cannot be found, updated or linked reliably."
                    .into(),
                "Add a primary key, for example on the id column.".into(),
            );
        }
        if let Some(pk) = &t.primary_key_name {
            if pk.to_lowercase().starts_with("fk") {
                bad_names.push(format!("{name}: primary key named {pk}"));
            }
        }

        for f in &t.foreign_keys {
            let fk_name = f.name.as_deref().unwrap_or("");
            let referenced = table_name(&f.references);
            let cols = f.columns.join(", ");
            if fk_name.to_lowercase().starts_with("pk") {
                bad_names.push(format!("{name}: link named {fk_name}"));
            }
            if f.not_valid {
                add(
                    "LINK-NOT-VALIDATED",
                    Severity::High,
                    "Links",
                    format!("{name}.{cols} → {referenced}"),
                    "Link rule was never checked for older rows".into(),
                    format!(
                        "The rule \"every {cols} must exist in {referenced}\" was added with NOT VALID. New rows are checked, but rows that already existed were never checked and may point to nothing."
                    ),
                    format!(
                        "Count the rows that break the rule. If there are none, run: {}",
                        crate::systems::validate_link(&schema.dbms, &t.schema, name, fk_name)
                    ),
                );
            }
            if let [column] = f.columns.as_slice() {
                let parts = link_name_parts(column);
                let mut best: Option<&(&Table, Vec<String>)> = None;
                for candidate in &by_name {
                    let n = candidate.1.len();
                    if n <= parts.len()
                        && parts[parts.len() - n..] == candidate.1[..]
                        && best.is_none_or(|b| n > b.1.len())
                    {
                        best = Some(candidate);
                    }
                }
                if let Some((best, _)) = best.filter(|(b, _)| b.key != f.references) {
                    let object = format!("{name}.{column} → {referenced}");
                    let suggested = &best.name;
                    if referenced
                        .to_lowercase()
                        .contains(&suggested.to_lowercase())
                    {
                        add(
                            "NAME-LINK-COLUMN",
                            Severity::Low,
                            "Naming",
                            object,
                            "Column name suggests a different table".into(),
                            format!(
                                "The column is called {column}, which sounds like a link to {suggested}, but it links to {referenced}. It works, but it confuses people and import scripts."
                            ),
                            "Rename the column or document the link in a column comment.".into(),
                        );
                    } else {
                        add(
                            "LINK-WRONG-TARGET",
                            Severity::Blocking,
                            "Links",
                            object,
                            "Link probably points to the wrong table".into(),
                            format!(
                                "From its name, {column} should point to {suggested}. The rule makes it point to {referenced} instead. Real {suggested} IDs will be rejected, so this link cannot be used."
                            ),
                            format!(
                                "Check with the team, then drop the rule \"{fk_name}\" and recreate it with REFERENCES {suggested}."
                            ),
                        );
                    }
                }
            }
        }

        if schema.has.indexes {
            let leading: Vec<Option<&String>> =
                t.indexes.iter().map(|i| i.columns.first()).collect();
            let missing: Vec<String> = t
                .foreign_keys
                .iter()
                .filter(|f| !leading.contains(&f.columns.first()))
                .map(|f| f.columns.join(", "))
                .collect();
            if !missing.is_empty() {
                let list = missing.join(", ");
                let title = if missing.len() == 1 {
                    "A link has no index".to_string()
                } else {
                    format!("{} links have no index", missing.len())
                };
                add(
                    "PERF-FK-NO-INDEX",
                    Severity::Low,
                    "Speed",
                    format!("{name}: {list}"),
                    title,
                    "Joins over these links and deletes of the linked rows get slow as the table grows."
                        .into(),
                    format!("Add an index on {list}."),
                );
            }
            let mut seen: Vec<(String, Vec<String>)> = Vec::new();
            for i in &t.indexes {
                let key = format!(
                    "{}{}",
                    i.columns.join(","),
                    if i.partial { "|p" } else { "" }
                );
                push_group(&mut seen, &key, i.name.clone().unwrap_or_default());
            }
            for (key, names) in seen.iter().filter(|(_, n)| n.len() > 1) {
                add(
                    "PERF-DUP-INDEX",
                    Severity::Low,
                    "Speed",
                    format!("{name}({})", key.replacen("|p", "", 1)),
                    "Same index exists twice".into(),
                    format!(
                        "Indexes {} cover the same columns. The extra one only slows down writes.",
                        names.join(" and ")
                    ),
                    "Drop one of them.".into(),
                );
            }
        }

        let (mut with_zone, mut without_zone) = (false, false);
        for c in &t.columns {
            let info = type_info(&c.data_type);
            let n = c.name.to_lowercase();
            if info.category == Category::Timestamp {
                if info.time_zone == Some(true) {
                    with_zone = true;
                } else {
                    without_zone = true;
                }
            }
            if let Some(default) = c
                .default
                .as_deref()
                .filter(|d| d.to_lowercase().contains("uuid"))
            {
                let generator = generator_name(default);
                if !generators.contains(&generator) {
                    generators.push(generator);
                }
            }
            if is_reserved(&n) {
                reserved_hits.push(format!("{name}.{}", c.name));
            }
            if looks_secret(&n) {
                let what = if n.contains("pass") || n == "pwd" {
                    "passwords"
                } else {
                    "secrets"
                };
                add(
                    "SEC-SENSITIVE",
                    Severity::High,
                    "Security",
                    format!("{name}.{}", c.name),
                    "Sensitive data".into(),
                    format!(
                        "This column looks like it stores {what}. It must never be copied to another system."
                    ),
                    "Exclude it from every export and import. Check that values are hashed.".into(),
                );
            }
            if info.array && info.category == Category::Uuid {
                add(
                    "LINK-ID-ARRAY",
                    Severity::Low,
                    "Links",
                    format!("{name}.{}", c.name),
                    "List of IDs cannot be checked".into(),
                    format!(
                        "{} stores several IDs in one field. The database cannot check that they exist, so deleted records stay listed.",
                        c.name
                    ),
                    "Use a link table (one row per ID) if these lists matter.".into(),
                );
            }
            if t.primary_key.contains(&c.name)
                || t.foreign_keys.iter().any(|f| f.columns.contains(&c.name))
            {
                continue;
            }
            let looks_like_link = n.ends_with("_id")
                || n.ends_with("_uuid")
                || n.starts_with("uuid_")
                || n.starts_with("fk_");
            let unique = is_unique_column(t, &c.name);
            if !has_text(&c.default)
                && !unique
                && (looks_like_link || (info.category == Category::Uuid && !info.array))
            {
                add(
                    "LINK-IMPLICIT",
                    Severity::Medium,
                    "Links",
                    format!("{name}.{}", c.name),
                    "Looks like a link, but nothing checks it".into(),
                    format!(
                        "{} seems to hold the ID of another record, but there is no rule (foreign key). It can point to records that do not exist.",
                        c.name
                    ),
                    "Count values with no matching record. If the link is real, add a foreign key."
                        .into(),
                );
            }
            if info.category == Category::Int && CODE_ENDINGS.iter().any(|e| n.ends_with(e)) {
                push_group(
                    &mut code_groups,
                    &n,
                    (name.to_string(), c.data_type.clone()),
                );
            }
            if IDENTIFIERS.contains(&n.as_str()) && !unique {
                add(
                    "DUP-IDENTIFIER",
                    Severity::Medium,
                    "Duplicates",
                    format!("{name}.{}", c.name),
                    "Identifier can be duplicated".into(),
                    format!(
                        "{} is the kind of value that should identify one record, but the database allows the same value twice.",
                        c.name
                    ),
                    format!(
                        "Check for duplicates, then add a unique rule on {}.",
                        c.name
                    ),
                );
            }
        }

        for u in &t.uniques {
            if let [column] = u.columns.as_slice() {
                if let Some(c) = t
                    .columns
                    .iter()
                    .find(|c| &c.name == column)
                    .filter(|c| c.nullable)
                {
                    add(
                        "DUP-NULLABLE-UNIQUE",
                        Severity::Low,
                        "Duplicates",
                        format!("{name}.{}", c.name),
                        "Unique column may be empty".into(),
                        format!(
                            "{} must be unique, but it can also be left empty, and empty values do not count as duplicates.",
                            c.name
                        ),
                        "Make it required if every row needs one.".into(),
                    );
                }
            }
        }

        if with_zone && without_zone {
            add(
                "TIME-MIXED-TZ",
                Severity::Medium,
                "Time",
                name.to_string(),
                "Two kinds of time in one table".into(),
                "Some time columns store a time zone and some do not. It is like writing one time in local time and another in UTC.".into(),
                "Use timestamp with time zone everywhere, or document which zone the others use."
                    .into(),
            );
        }

        let mut stems: Vec<(String, Vec<String>)> = Vec::new();
        for c in &t.columns {
            let n = c.name.to_lowercase();
            if let Some((stem, ending)) = n.rsplit_once('_') {
                if FORMAT_ENDINGS.contains(&ending) {
                    push_group(&mut stems, stem, c.name.clone());
                }
            }
        }
        for (_, names) in stems.iter().filter(|(_, n)| n.len() > 1) {
            add(
                "DATA-MULTI-FORMAT",
                Severity::Medium,
                "Consistency",
                format!("{name}: {}", names.join(", ")),
                "Same data saved in several formats".into(),
                format!(
                    "The same information is saved {} times in different formats. If the copies disagree, nobody knows which one is right.",
                    names.len()
                ),
                "Decide which format is the master copy and compare the others with it before migrating.".into(),
            );
        }

        if t.primary_key.len() > 1
            && t.columns
                .iter()
                .any(|c| c.name == "id" && !t.primary_key.contains(&c.name))
            && !is_unique_column(t, "id")
        {
            add(
                "KEY-SURROGATE-NOT-UNIQUE",
                Severity::Medium,
                "Keys",
                format!("{name}.id"),
                "Extra id column is not unique".into(),
                format!(
                    "The real key is ({}). There is also an id column, but nothing stops two rows from having the same id.",
                    t.primary_key.join(", ")
                ),
                "Make id unique, or do not use it as an identifier.".into(),
            );
        }

        match t.rows {
            Some(0) => empty_tables.push(name.to_string()),
            Some(r) if r < 0 => unanalysed.push(name.to_string()),
            _ => {}
        }
    }

    for (code, uses) in code_groups.iter().filter(|(_, g)| g.len() > 1) {
        let mut types: Vec<&str> = Vec::new();
        for (_, ty) in uses {
            if !types.contains(&ty.as_str()) {
                types.push(ty);
            }
        }
        let tables: Vec<&str> = uses.iter().map(|(t, _)| t.as_str()).collect();
        let different = if types.len() > 1 {
            format!(" It also has different types: {}.", types.join(" / "))
        } else {
            String::new()
        };
        add(
            "LINK-SHARED-CODE",
            Severity::Medium,
            "Links",
            format!("{code} in {}", tables.join(", ")),
            "Shared code with no lookup list".into(),
            format!(
                "{code} appears in {} tables as a plain number. There is no table that lists the allowed values, so wrong or empty values are possible.{different}",
                uses.len()
            ),
            format!("Create a small lookup table of valid {code} values and link to it."),
        );
    }
    if !reserved_hits.is_empty() {
        add(
            "NAME-RESERVED",
            Severity::Low,
            "Naming",
            reserved_hits.join(", "),
            "Names that need quotes in SQL".into(),
            "These names are reserved words in SQL. Every query must write them in double quotes, which is easy to forget.".into(),
            "Rename them when convenient, or always quote them.".into(),
        );
    }
    if !bad_names.is_empty() {
        add(
            "NAME-MISLEADING",
            Severity::Low,
            "Naming",
            bad_names.join("; "),
            "Misleading rule names".into(),
            "Some primary keys are named like links (fk…) or links are named like primary keys (pk…).".into(),
            "Rename the rules so the name matches what they do.".into(),
        );
    }
    if generators.len() > 1 {
        add(
            "KEY-MIXED-GENERATORS",
            Severity::Low,
            "Keys",
            generators.join(", "),
            "Different ID generators".into(),
            "Tables create their IDs with different functions. It works, but IDs sort differently from table to table.".into(),
            "Pick one generator for new tables.".into(),
        );
    }
    if !empty_tables.is_empty() {
        let title = if empty_tables.len() > 1 {
            format!("{} tables are empty", empty_tables.len())
        } else {
            "1 table is empty".to_string()
        };
        add(
            "CONTENT-EMPTY",
            Severity::Info,
            "Content",
            empty_tables.join(", "),
            title,
            "These tables have no rows (according to database statistics), so there is nothing to migrate from them right now.".into(),
            "Confirm they are unused before planning work on them.".into(),
        );
    }
    if !unanalysed.is_empty() {
        add(
            "CONTENT-EMPTY",
            Severity::Info,
            "Content",
            unanalysed.join(", "),
            "Row counts unknown".into(),
            "The database has never collected statistics for these tables, so their size is unknown. They are probably empty.".into(),
            "Run ANALYZE, or count the rows.".into(),
        );
    }
    if let Some(fix) = crate::systems::description_fix(&schema.dbms) {
        let percent = if columns > 0 {
            (100.0 * columns_documented as f64 / columns as f64).round() as u32
        } else {
            0
        };
        add(
            "DOC-COVERAGE",
            if percent < 30 { Severity::Low } else { Severity::Info },
            "Descriptions",
            format!(
                "{tables_documented} of {} tables, {columns_documented} of {columns} columns",
                schema.tables.len()
            ),
            format!("Descriptions on {percent}% of columns"),
            "Few columns explain what they mean. People have to guess, and matching with another database gets harder.".into(),
            fix.into(),
        );
    }
    out
}

/// How many findings there are of each severity, in [`Severity::ALL`] order.
pub fn count_by_severity<'a>(findings: impl IntoIterator<Item = &'a Finding>) -> [usize; 5] {
    let mut counts = [0; 5];
    for f in findings {
        counts[f.severity as usize] += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Column, ForeignKey, Index, Provided, UniqueKey};

    fn column(name: &str, data_type: &str) -> Column {
        Column {
            name: name.into(),
            type_label: data_type.into(),
            data_type: data_type.into(),
            nullable: true,
            ..Default::default()
        }
    }

    fn table(name: &str, columns: Vec<Column>) -> Table {
        Table {
            key: format!("public.{name}"),
            schema: "public".into(),
            name: name.into(),
            columns,
            primary_key: vec!["id".into()],
            rows: Some(10),
            ..Default::default()
        }
    }

    fn schema(tables: Vec<Table>) -> Schema {
        Schema {
            dbms: "postgresql".into(),
            tables,
            has: Provided {
                columns: true,
                constraints: true,
                indexes: true,
                row_counts: true,
            },
            ..Default::default()
        }
    }

    fn titles(s: &Schema) -> Vec<String> {
        check_schema(s).into_iter().map(|f| f.title).collect()
    }

    #[test]
    fn empty_schema_has_no_findings() {
        assert!(check_schema(&Schema::default()).is_empty());
    }

    #[test]
    fn missing_primary_key_only_when_constraints_were_exported() {
        let mut t = table("note", vec![column("id", "integer")]);
        t.primary_key.clear();
        let mut s = schema(vec![t]);
        assert!(titles(&s).contains(&"Table has no primary key".to_string()));
        s.has.constraints = false;
        assert!(!titles(&s).contains(&"Table has no primary key".to_string()));
    }

    #[test]
    fn link_to_the_wrong_table_is_blocking() {
        let mut item = table(
            "item",
            vec![column("id", "integer"), column("box_id", "integer")],
        );
        item.foreign_keys.push(ForeignKey {
            name: Some("item_box_fkey".into()),
            columns: vec!["box_id".into()],
            references: "public.shelf".into(),
            referenced_columns: vec!["id".into()],
            ..Default::default()
        });
        item.indexes.push(Index {
            name: Some("ix_item_box".into()),
            columns: vec!["box_id".into()],
            ..Default::default()
        });
        let s = schema(vec![
            item,
            table("box", vec![column("id", "integer")]),
            table("shelf", vec![column("id", "integer")]),
        ]);
        let found = check_schema(&s);
        assert_eq!(found.len(), 2, "{found:#?}"); // plus the description coverage
        assert_eq!(found[0].severity, Severity::Blocking);
        assert_eq!(found[0].object, "item.box_id → shelf");
        assert!(found[0].fix.contains("REFERENCES box"));
    }

    #[test]
    fn unchecked_link_column_and_duplicate_identifier() {
        let s = schema(vec![table(
            "device",
            vec![
                column("id", "integer"),
                column("owner_id", "integer"),
                column("email", "text"),
                column("api_key", "text"),
            ],
        )]);
        let t = titles(&s);
        assert!(t.contains(&"Looks like a link, but nothing checks it".to_string()));
        assert!(t.contains(&"Identifier can be duplicated".to_string()));
        assert!(t.contains(&"Sensitive data".to_string()));
    }

    #[test]
    fn unique_rules_remove_the_duplicate_finding() {
        let mut t = table(
            "device",
            vec![column("id", "integer"), column("email", "text")],
        );
        t.uniques.push(UniqueKey {
            name: None,
            columns: vec!["email".into()],
        });
        let titles = titles(&schema(vec![t]));
        assert!(!titles.contains(&"Identifier can be duplicated".to_string()));
        // The unique column may still be empty.
        assert!(titles.contains(&"Unique column may be empty".to_string()));
    }

    #[test]
    fn mixed_time_zones_and_reserved_words() {
        let s = schema(vec![table(
            "event",
            vec![
                column("id", "integer"),
                column("start", "timestamp with time zone"),
                column("end", "timestamp without time zone"),
            ],
        )]);
        let found = check_schema(&s);
        assert!(
            found
                .iter()
                .any(|f| f.title == "Two kinds of time in one table")
        );
        let reserved = found.iter().find(|f| f.area == "Naming").unwrap();
        assert_eq!(reserved.object, "event.end");
    }

    #[test]
    fn duplicate_index_and_missing_link_index() {
        let mut t = table(
            "run",
            vec![column("id", "integer"), column("machine_id", "integer")],
        );
        t.foreign_keys.push(ForeignKey {
            name: Some("run_machine_fkey".into()),
            columns: vec!["machine_id".into()],
            references: "public.machine".into(),
            ..Default::default()
        });
        for name in ["ix_a", "ix_b"] {
            t.indexes.push(Index {
                name: Some(name.into()),
                columns: vec!["id".into()],
                ..Default::default()
            });
        }
        let s = schema(vec![t, table("machine", vec![column("id", "integer")])]);
        let found = check_schema(&s);
        let speed: Vec<&str> = found
            .iter()
            .filter(|f| f.area == "Speed")
            .map(|f| f.title.as_str())
            .collect();
        assert_eq!(speed, ["A link has no index", "Same index exists twice"]);
    }

    #[test]
    fn row_counts_and_description_coverage() {
        let mut a = table("a", vec![column("id", "integer")]);
        a.rows = Some(0);
        let mut b = table("b", vec![column("id", "integer")]);
        b.rows = Some(-1);
        b.columns[0].comment = Some("The ID.".into());
        let found = check_schema(&schema(vec![a, b]));
        let titles: Vec<&str> = found.iter().map(|f| f.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "1 table is empty",
                "Row counts unknown",
                "Descriptions on 50% of columns"
            ]
        );
        assert_eq!(found[2].severity, Severity::Info);
    }

    #[test]
    fn sqlite_has_no_description_finding() {
        let mut s = schema(vec![table("a", vec![column("id", "integer")])]);
        s.dbms = "sqlite".into();
        assert!(check_schema(&s).is_empty());
    }

    #[test]
    fn helpers() {
        assert_eq!(quote("asset"), "asset");
        assert_eq!(quote("end"), "\"end\"");
        assert_eq!(quote("Asset"), "\"Asset\"");
        assert!(looks_secret("password") && looks_secret("user_token"));
        assert!(!looks_secret("bypass") && !looks_secret("tokens"));
        assert_eq!(link_name_parts("fk_parent_asset_id"), ["parent", "asset"]);
        assert_eq!(generator_name("gen_random_uuid()"), "gen_random_uuid");
        let counts = count_by_severity(&check_schema(&schema(vec![table(
            "x",
            vec![column("id", "integer"), column("pwd", "text")],
        )])));
        assert_eq!(counts, [0, 1, 0, 1, 0]);
    }
}
