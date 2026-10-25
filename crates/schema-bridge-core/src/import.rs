//! Reading structure exports into a [`Schema`].
//!
//! Ported from the prototype's `kindOf`, `withHeader`, `normType`,
//! `listCols`, `buildModel` and the file handling in `readFiles`.
//! Two export formats are supported, as in the prototype:
//!
//! * the one-file export (one query per database system, 21 columns,
//!   `record_type` says what each row describes), and
//! * the older four-file PostgreSQL export (columns, constraints, indexes
//!   and row counts in separate files).

use std::collections::HashMap;
use std::fmt;

use crate::csv_reader::{self, Record, get, get_nonempty};
use crate::model::{
    Check, Column, ColumnRole, ForeignKey, Index, Provided, Schema, Table, UniqueKey,
};

/// Header of the one-file export, used when a tool saved it without headers.
const COMBINED_HEADER: &str = "record_type,table_schema,table_name,object_type,ordinal_position,column_name,data_type,is_nullable,column_default,table_comment,column_comment,constraint_name,constraint_type,definition,ref_schema,ref_table,indexname,indexdef,estimated_rows,triggers,dbms";

/// What an export file contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    /// The one-file export with everything in it.
    Combined,
    Columns,
    Constraints,
    Indexes,
    RowCounts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    /// The file does not look like any supported export.
    Unrecognised,
    /// The file is a recognised export, but one of its lines is invalid.
    Invalid { line: usize, reason: String },
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportError::Unrecognised => write!(
                f,
                "This file is not a Schema Bridge export. Use the queries under \"How to make an export file\"."
            ),
            ImportError::Invalid { line, reason } => write!(f, "Line {line}: {reason}"),
        }
    }
}

impl std::error::Error for ImportError {}

/// The export files loaded for one database (one "slot" in the UI).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportFiles {
    pub combined: Option<String>,
    pub columns: Option<String>,
    pub constraints: Option<String>,
    pub indexes: Option<String>,
    pub row_counts: Option<String>,
}

impl ExportFiles {
    /// Adds one file. A one-file export replaces everything loaded before;
    /// a part of the four-file export replaces only that part and removes
    /// any one-file export. This is how the prototype's slots behave.
    pub fn add(&mut self, text: impl Into<String>) -> Result<ExportKind, ImportError> {
        let text = text.into();
        let kind = detect_kind(&text).ok_or(ImportError::Unrecognised)?;
        match kind {
            ExportKind::Combined => {
                *self = ExportFiles {
                    combined: Some(with_header(&text)),
                    ..Default::default()
                };
            }
            other => {
                self.combined = None;
                *self.part_mut(other) = Some(text);
            }
        }
        Ok(kind)
    }

    pub fn is_empty(&self) -> bool {
        *self == ExportFiles::default()
    }

    /// Whether the given part is available, directly or inside a one-file export.
    pub fn has(&self, kind: ExportKind) -> bool {
        self.combined.is_some()
            || match kind {
                ExportKind::Combined => false,
                ExportKind::Columns => self.columns.is_some(),
                ExportKind::Constraints => self.constraints.is_some(),
                ExportKind::Indexes => self.indexes.is_some(),
                ExportKind::RowCounts => self.row_counts.is_some(),
            }
    }

    fn part_mut(&mut self, kind: ExportKind) -> &mut Option<String> {
        match kind {
            ExportKind::Combined => &mut self.combined,
            ExportKind::Columns => &mut self.columns,
            ExportKind::Constraints => &mut self.constraints,
            ExportKind::Indexes => &mut self.indexes,
            ExportKind::RowCounts => &mut self.row_counts,
        }
    }
}

/// Recognises an export file from its first lines (its header).
pub fn detect_kind(text: &str) -> Option<ExportKind> {
    let start: Vec<&str> = text.split('\n').take(3).collect();
    let header = csv_reader::parse(&start.join("\n")).header;
    let has = |name: &str| header.iter().any(|h| h == name);
    let first = header.first().map(String::as_str).unwrap_or("");
    if has("record_type") || matches!(first, "column" | "constraint" | "index" | "table") {
        Some(ExportKind::Combined)
    } else if has("indexdef") {
        Some(ExportKind::Indexes)
    } else if has("constraint_type") {
        Some(ExportKind::Constraints)
    } else if has("estimated_rows") {
        Some(ExportKind::RowCounts)
    } else if has("column_name") && has("data_type") {
        Some(ExportKind::Columns)
    } else {
        None
    }
}

/// Adds the header to a one-file export that was saved without one.
fn with_header(text: &str) -> String {
    let start: String = text
        .strip_prefix('\u{feff}')
        .unwrap_or(text)
        .trim_start()
        .chars()
        .take(40)
        .collect();
    let start = start.strip_prefix('"').unwrap_or(&start).to_lowercase();
    let headerless = ["column", "constraint", "index", "table"]
        .iter()
        .any(|kind| {
            start.strip_prefix(kind).is_some_and(|rest| {
                let rest = rest.trim_start();
                let rest = rest.strip_prefix('"').unwrap_or(rest);
                rest.starts_with(',') || rest.starts_with(';')
            })
        });
    if headerless {
        format!("{COMBINED_HEADER}\n{text}")
    } else {
        text.to_string()
    }
}

/// Builds the schema model from the loaded export files.
pub fn build_schema(files: &ExportFiles) -> Schema {
    let read = |text: &Option<String>| text.as_deref().map(|t| csv_reader::parse(t).rows);
    let mut columns = read(&files.columns);
    let mut constraints = read(&files.constraints);
    let mut indexes = read(&files.indexes);
    let mut row_counts = read(&files.row_counts);
    let mut dbms = String::from("postgresql");

    if let Some(text) = &files.combined {
        let all = csv_reader::parse(&with_header(text)).rows;
        if let Some(d) = all.iter().find_map(|r| get_nonempty(r, "dbms")) {
            dbms = d.trim().to_lowercase();
        }
        let of = |kind: &str| -> Vec<Record> {
            all.iter()
                .filter(|r| get(r, "record_type").map(str::trim) == Some(kind))
                .cloned()
                .collect()
        };
        columns = Some(of("column"));
        constraints = Some(of("constraint"));
        indexes = Some(of("index"));
        row_counts = Some(of("table"));
    }

    let mut tables: Vec<Table> = Vec::new();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    let mut views = std::collections::BTreeSet::new();

    for c in columns.iter().flatten() {
        let Some(name) = get_nonempty(c, "table_name") else {
            continue;
        };
        if name.to_lowercase().starts_with("databasechangelog") {
            continue; // Liquibase bookkeeping tables
        }
        let key = table_key(get_nonempty(c, "table_schema"), name);
        let object_type = get_nonempty(c, "object_type")
            .unwrap_or("table")
            .to_lowercase();
        if object_type.contains("view") {
            views.insert(key);
            continue;
        }
        let index = *by_key.entry(key.clone()).or_insert_with(|| {
            tables.push(Table {
                key,
                schema: get_nonempty(c, "table_schema")
                    .unwrap_or("public")
                    .to_string(),
                name: name.to_string(),
                ..Default::default()
            });
            tables.len() - 1
        });
        let table = &mut tables[index];
        let label = get_nonempty(c, "data_type").unwrap_or("");
        let position = get(c, "ordinal_position")
            .and_then(|p| p.trim().parse::<f64>().ok())
            .filter(|p| p.is_finite() && *p != 0.0)
            .map(|p| p as i64)
            .unwrap_or(table.columns.len() as i64 + 1);
        table.columns.push(Column {
            name: get(c, "column_name").unwrap_or("").to_string(),
            type_label: label.to_string(),
            data_type: crate::systems::normalise_type(label, &dbms),
            nullable: is_true(get(c, "is_nullable")),
            default: get(c, "column_default").map(str::to_string),
            comment: get(c, "column_comment").map(str::to_string),
            position,
            // The one-file export describes relational tables only.
            role: ColumnRole::Regular,
        });
        if table.comment.as_deref().is_none_or(str::is_empty) {
            if let Some(comment) = get_nonempty(c, "table_comment") {
                table.comment = Some(comment.to_string());
            }
        }
    }

    for c in constraints.iter().flatten() {
        let key = table_key(
            get_nonempty(c, "table_schema"),
            get(c, "table_name").unwrap_or(""),
        );
        let Some(&i) = by_key.get(&key) else { continue };
        let table = &mut tables[i];
        let definition = get_nonempty(c, "definition").unwrap_or("").to_string();
        let kind = get_nonempty(c, "constraint_type")
            .unwrap_or("")
            .to_uppercase();
        let name = get(c, "constraint_name").map(str::to_string);
        if kind.starts_with("PRIMARY") {
            table.primary_key = list_columns(between(&definition, "PRIMARY KEY (", ")"));
            table.primary_key_name = name;
        } else if kind.starts_with("UNIQUE") {
            table.uniques.push(UniqueKey {
                name,
                columns: list_columns(between(&definition, "UNIQUE (", ")")),
            });
        } else if kind.starts_with("FOREIGN") {
            let parsed = parse_foreign_key(&definition);
            let mut ref_schema = get_nonempty(c, "ref_schema").map(str::to_string);
            let mut ref_table = get_nonempty(c, "ref_table").map(str::to_string);
            if ref_table.is_none() {
                if let Some((_, target, _)) = &parsed {
                    let mut parts: Vec<String> =
                        target.split('.').map(|p| p.replace('"', "")).collect();
                    ref_table = parts.pop();
                    ref_schema = parts.pop();
                }
            }
            let (cols, _, ref_cols) = parsed.unwrap_or_default();
            table.foreign_keys.push(ForeignKey {
                name,
                columns: list_columns(Some(&cols)),
                references: table_key(
                    Some(ref_schema.as_deref().unwrap_or(&table.schema)),
                    ref_table.as_deref().unwrap_or(""),
                ),
                referenced_columns: list_columns(Some(&ref_cols)),
                not_valid: definition.contains("NOT VALID"),
                on_delete: on_delete(&definition),
                definition,
            });
        } else if kind.starts_with("CHECK") {
            table.checks.push(Check { name, definition });
        }
    }

    for i in indexes.iter().flatten() {
        let key = table_key(
            get_nonempty(i, "table_schema"),
            get(i, "table_name").unwrap_or(""),
        );
        let Some(&t) = by_key.get(&key) else { continue };
        let definition = get_nonempty(i, "indexdef").unwrap_or("").to_string();
        let upper = definition.to_uppercase();
        tables[t].indexes.push(Index {
            name: get(i, "indexname").map(str::to_string),
            unique: upper.contains("CREATE UNIQUE"),
            partial: has_word_between_spaces(&upper, "WHERE"),
            columns: list_columns(index_columns(&definition)),
            definition,
        });
    }

    for r in row_counts.iter().flatten() {
        let key = table_key(
            get_nonempty(r, "table_schema"),
            get(r, "table_name").unwrap_or(""),
        );
        let Some(&t) = by_key.get(&key) else { continue };
        tables[t].rows = get(r, "estimated_rows")
            .and_then(|v| v.trim().parse::<f64>().ok())
            .map(|v| v as i64);
        tables[t].triggers = get_nonempty(r, "triggers").unwrap_or("").to_string();
    }

    // The prototype sorts with the browser's locale order; a case-insensitive
    // order gives the same result for normal table names.
    tables.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    });
    for table in &mut tables {
        table.columns.sort_by_key(|c| c.position);
    }

    Schema {
        dbms,
        tables,
        views,
        retention_policies: Vec::new(),
        has: Provided {
            columns: columns.is_some(),
            constraints: constraints.is_some(),
            indexes: indexes.is_some(),
            row_counts: row_counts.is_some(),
        },
    }
}

/// `schema.name`, with `public` when the schema is missing.
fn table_key(schema: Option<&str>, name: &str) -> String {
    format!(
        "{}.{}",
        schema.filter(|s| !s.is_empty()).unwrap_or("public"),
        name
    )
}

/// The prototype's `bool`: true, t, yes, y or 1 (any case) mean true.
fn is_true(value: Option<&str>) -> bool {
    matches!(
        value.map(str::to_lowercase).as_deref(),
        Some("true" | "t" | "yes" | "y" | "1")
    )
}

/// Splits a column list like `a, "b", c` into names without quotes.
fn list_columns(list: Option<&str>) -> Vec<String> {
    list.unwrap_or("")
        .split(',')
        .map(|c| {
            let c = c.trim();
            let c = c.strip_prefix('"').unwrap_or(c);
            c.strip_suffix('"').unwrap_or(c).to_string()
        })
        .filter(|c| !c.is_empty())
        .collect()
}

/// The text after `start` up to the first `end`.
fn between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let from = text.find(start)? + start.len();
    let len = text[from..].find(end)?;
    Some(&text[from..from + len])
}

/// Splits `FOREIGN KEY (cols) REFERENCES target(ref_cols)` into its three parts.
fn parse_foreign_key(definition: &str) -> Option<(String, String, String)> {
    let from = definition.find("FOREIGN KEY (")? + "FOREIGN KEY (".len();
    let rest = &definition[from..];
    let cols_end = rest.find(") REFERENCES ")?;
    let cols = &rest[..cols_end];
    let rest = &rest[cols_end + ") REFERENCES ".len()..];
    // The target needs at least one character before its "(".
    let target_end = rest.char_indices().skip(1).find(|(_, c)| *c == '(')?.0;
    let target = &rest[..target_end];
    let rest = &rest[target_end + 1..];
    let ref_cols = &rest[..rest.find(')')?];
    Some((cols.to_string(), target.to_string(), ref_cols.to_string()))
}

/// The delete rule written in a constraint definition, if it is not "no action".
fn on_delete(definition: &str) -> Option<String> {
    definition.match_indices("ON DELETE ").find_map(|(i, m)| {
        let rest = &definition[i + m.len()..];
        ["CASCADE", "SET NULL", "SET DEFAULT", "RESTRICT"]
            .into_iter()
            .find(|rule| rest.starts_with(rule))
            .map(str::to_string)
    })
}

/// True when `word` stands between whitespace, like ` WHERE ` in an index definition.
fn has_word_between_spaces(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + word.len()..].chars().next();
        before.is_some_and(char::is_whitespace) && after.is_some_and(char::is_whitespace)
    })
}

/// The column list of an index definition: the first `( … )` after ` ON `
/// that is followed by `INCLUDE`, `WHERE` or the end, as in the prototype.
fn index_columns(definition: &str) -> Option<&str> {
    let start = definition
        .char_indices()
        .find(|&(i, c)| {
            c.is_whitespace()
                && definition[i + c.len_utf8()..].starts_with("ON")
                && definition[i + c.len_utf8() + 2..]
                    .chars()
                    .next()
                    .is_some_and(char::is_whitespace)
        })
        .map_or(0, |(i, _)| i);
    let text = &definition[start..];
    for (open, _) in text.match_indices('(') {
        for (close, _) in text[open + 1..].match_indices(')') {
            let close = open + 1 + close;
            let inner = &text[open + 1..close];
            if inner.contains('\n') {
                break;
            }
            let after = &text[close + 1..];
            let trimmed = after.trim_start();
            let spaced = trimmed.len() < after.len();
            if trimmed.is_empty()
                || (spaced && (trimmed.starts_with("INCLUDE") || trimmed.starts_with("WHERE")))
            {
                return Some(inner);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_every_export_kind_from_its_header() {
        assert_eq!(
            detect_kind("record_type,table_schema\ncolumn,public"),
            Some(ExportKind::Combined)
        );
        assert_eq!(
            detect_kind("column,public,asset,table,1"),
            Some(ExportKind::Combined)
        );
        assert_eq!(
            detect_kind("table_schema,table_name,indexname,indexdef"),
            Some(ExportKind::Indexes)
        );
        assert_eq!(
            detect_kind("table_schema,table_name,constraint_name,constraint_type"),
            Some(ExportKind::Constraints)
        );
        assert_eq!(
            detect_kind("table_schema,table_name,estimated_rows"),
            Some(ExportKind::RowCounts)
        );
        assert_eq!(
            detect_kind("table_schema,table_name,column_name,data_type"),
            Some(ExportKind::Columns)
        );
        assert_eq!(detect_kind("name,price\nbread,2"), None);
    }

    #[test]
    fn adding_files_replaces_like_the_prototype_slots() {
        let mut files = ExportFiles::default();
        assert_eq!(
            files.add("table_schema,table_name,column_name,data_type\n"),
            Ok(ExportKind::Columns)
        );
        assert!(files.has(ExportKind::Columns) && !files.has(ExportKind::Indexes));
        files.add("record_type,table_name\n").unwrap();
        assert!(files.columns.is_none() && files.has(ExportKind::Indexes));
        files
            .add("table_schema,table_name,estimated_rows\n")
            .unwrap();
        assert!(files.combined.is_none() && files.row_counts.is_some());
        assert_eq!(files.add("not,an,export\n"), Err(ImportError::Unrecognised));
    }

    #[test]
    fn adds_the_header_to_a_headerless_one_file_export() {
        assert!(with_header("column,public,a\n").starts_with("record_type,"));
        assert!(with_header("\"table\";main;b\n").starts_with("record_type,"));
        assert_eq!(with_header("record_type,x\n"), "record_type,x\n");
        assert_eq!(with_header("columnist,x\n"), "columnist,x\n");
    }

    #[test]
    fn lists_columns_without_quotes_or_spaces() {
        assert_eq!(list_columns(Some("a, \"b\" ,c")), ["a", "b", "c"]);
        assert!(list_columns(None).is_empty());
    }

    #[test]
    fn reads_foreign_keys_with_and_without_schema() {
        let (cols, target, refs) = parse_foreign_key(
            "FOREIGN KEY (machine_id) REFERENCES \"Workshop\".machine(id) NOT VALID",
        )
        .unwrap();
        assert_eq!(
            (cols.as_str(), target.as_str(), refs.as_str()),
            ("machine_id", "\"Workshop\".machine", "id")
        );
        assert_eq!(
            on_delete("FOREIGN KEY (a) REFERENCES t(id) ON DELETE SET NULL"),
            Some("SET NULL".into())
        );
        assert_eq!(on_delete("FOREIGN KEY (a) REFERENCES t(id)"), None);
    }

    #[test]
    fn reads_index_columns_like_the_prototype() {
        let plain = "CREATE INDEX ix ON public.run USING btree (machine_id)";
        assert_eq!(index_columns(plain), Some("machine_id"));
        let partial = "CREATE UNIQUE INDEX u ON t USING btree (a, b) WHERE (match_type = 'x'::t)";
        assert_eq!(index_columns(partial), Some("a, b"));
        let included = "CREATE INDEX i ON t USING btree (a) INCLUDE (b)";
        assert_eq!(index_columns(included), Some("a"));
        let expression = "CREATE UNIQUE INDEX d ON t USING btree (lower(doi))";
        assert_eq!(index_columns(expression), Some("lower(doi)"));
        let other_system = "CREATE UNIQUE INDEX ux ON machine (serial_no)";
        assert_eq!(index_columns(other_system), Some("serial_no"));
        assert!(has_word_between_spaces(&partial.to_uppercase(), "WHERE"));
        assert!(!has_word_between_spaces(plain, "WHERE"));
    }

    #[test]
    fn normalises_types_that_mean_something_else_per_system() {
        use crate::systems::normalise_type;
        assert_eq!(
            normalise_type("DATE", "oracle"),
            "timestamp(0) without time zone"
        );
        assert_eq!(normalise_type("DATE", "postgresql"), "date");
        assert_eq!(normalise_type("timestamp", "sqlserver"), "bytea");
        assert_eq!(normalise_type("float", "mysql"), "real");
        assert_eq!(
            normalise_type(" Character Varying(100) ", "postgresql"),
            "character varying(100)"
        );
    }

    #[test]
    fn reads_yes_no_values_like_the_prototype() {
        for yes in ["true", "T", "yes", "Y", "1"] {
            assert!(is_true(Some(yes)), "{yes}");
        }
        for no in ["false", "f", "NO", "0", ""] {
            assert!(!is_true(Some(no)), "{no}");
        }
        assert!(!is_true(None));
    }
}
