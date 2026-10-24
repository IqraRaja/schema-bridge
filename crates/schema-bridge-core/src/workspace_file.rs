//! Saving and reopening a comparison (FR-04): both databases' inputs as they
//! were loaded, the database names, the word list and every choice made by
//! hand. The file is JSON and stays on the user's computer; it holds
//! structure only (no table contents) and never a password.
//!
//! Format version 2 records, for each database, the importer that read it
//! and that importer's input. Version 1 files, which held the CSV export
//! parts directly, still open.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::input::{CSV_EXPORT, Importer, SchemaInput, importer};
use crate::matching::{Overrides, TableChoice};
use crate::workspace::{Slot, Workspace};

/// Written into every file, so other JSON files are recognised.
const FORMAT: &str = "schema-bridge-workspace";
/// Raised when the file layout changes in a way older versions cannot read.
/// 1: CSV export parts in each slot. 2: importer id and its input per slot.
const VERSION: u32 = 2;

#[derive(Serialize, Deserialize)]
struct File {
    format: String,
    version: u32,
    /// A slot: `{name, importer, input}` (version 2) or `{name, combined,
    /// columns, …}` (version 1). Read by hand to support both.
    source: Value,
    target: Value,
    #[serde(default)]
    using_samples: bool,
    #[serde(default)]
    word_list: String,
    #[serde(default)]
    table_choices: Vec<TableChoiceFile>,
    #[serde(default)]
    column_choices: Vec<ColumnChoiceFile>,
}

#[derive(Serialize, Deserialize)]
struct TableChoiceFile {
    source_table: String,
    /// `target`, `none` or `stays`.
    choice: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target_table: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ColumnChoiceFile {
    source_table: String,
    source_column: String,
    /// The target column; missing means "not copied".
    #[serde(default)]
    target_column: Option<String>,
}

/// Why a saved comparison could not be opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    NotJson,
    NotAWorkspace,
    NewerVersion(u32),
    /// A database was read by an importer this version does not have.
    UnknownImporter(String),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::NotJson | OpenError::NotAWorkspace => {
                write!(f, "This file is not a Schema Bridge workspace.")
            }
            OpenError::NewerVersion(v) => write!(
                f,
                "This workspace was saved by a newer version of Schema Bridge (format {v}). Update Schema Bridge to open it."
            ),
            OpenError::UnknownImporter(id) => write!(
                f,
                "This workspace contains a database read with \"{id}\", an import format this version of Schema Bridge does not support. Update Schema Bridge, or load that database again from its export file."
            ),
        }
    }
}

impl std::error::Error for OpenError {}

fn slot_file(slot: &Slot) -> Value {
    match slot.input.importer() {
        Some(i) => json!({ "name": slot.name, "importer": i.id(), "input": i.save(&slot.input) }),
        None => json!({ "name": slot.name }),
    }
}

/// Reads a slot. Version 1 slots hold the CSV export parts directly.
fn slot_from(value: &Value, version: u32, fallback: &str) -> Result<Slot, OpenError> {
    let object = value.as_object().ok_or(OpenError::NotAWorkspace)?;
    let name = match object.get("name") {
        Some(Value::String(n)) if !n.trim().is_empty() => n.clone(),
        None | Some(Value::String(_)) => fallback.to_string(),
        Some(_) => return Err(OpenError::NotAWorkspace),
    };
    let input = if version <= 1 {
        CSV_EXPORT.open(value).ok_or(OpenError::NotAWorkspace)?
    } else {
        match object.get("importer") {
            None | Some(Value::Null) => SchemaInput::Empty,
            Some(Value::String(id)) => {
                let importer =
                    importer(id).ok_or_else(|| OpenError::UnknownImporter(id.clone()))?;
                let input = object.get("input").cloned().unwrap_or_else(|| json!({}));
                importer.open(&input).ok_or(OpenError::NotAWorkspace)?
            }
            Some(_) => return Err(OpenError::NotAWorkspace),
        }
    };
    Ok(Slot {
        name,
        schema: input.build(),
        input,
        notice: None,
    })
}

/// Writes a workspace as pretty-printed JSON (format version 2).
pub fn save_workspace(ws: &Workspace) -> String {
    let file = File {
        format: FORMAT.into(),
        version: VERSION,
        source: slot_file(&ws.source),
        target: slot_file(&ws.target),
        using_samples: ws.using_samples,
        word_list: ws.word_list.clone(),
        table_choices: ws
            .overrides
            .tables
            .iter()
            .map(|(table, choice)| {
                let (choice, target_table) = match choice {
                    TableChoice::Target(t) => ("target", Some(t.clone())),
                    TableChoice::NoTarget => ("none", None),
                    TableChoice::Stays => ("stays", None),
                };
                TableChoiceFile {
                    source_table: table.clone(),
                    choice: choice.into(),
                    target_table,
                }
            })
            .collect(),
        column_choices: ws
            .overrides
            .columns
            .iter()
            .map(|((table, column), target)| ColumnChoiceFile {
                source_table: table.clone(),
                source_column: column.clone(),
                target_column: target.clone(),
            })
            .collect(),
    };
    serde_json::to_string_pretty(&file).expect("a workspace always serialises") + "\n"
}

/// Reads a workspace saved with [`save_workspace`], in format version 1 or
/// 2. Choices of an unknown kind are skipped; choices for tables that no
/// longer exist are kept and simply have no effect, as in the running
/// application.
pub fn open_workspace(text: &str) -> Result<Workspace, OpenError> {
    let value: Value = serde_json::from_str(text).map_err(|_| OpenError::NotJson)?;
    if value.get("format").and_then(|f| f.as_str()) != Some(FORMAT) {
        return Err(OpenError::NotAWorkspace);
    }
    let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    if version > VERSION {
        return Err(OpenError::NewerVersion(version));
    }
    let file: File = serde_json::from_value(value).map_err(|_| OpenError::NotAWorkspace)?;
    let source = slot_from(&file.source, version, "Source")?;
    let target = slot_from(&file.target, version, "Target")?;

    let mut overrides = Overrides::default();
    for c in file.table_choices {
        let choice = match (c.choice.as_str(), c.target_table) {
            ("target", Some(t)) => TableChoice::Target(t),
            ("none", _) => TableChoice::NoTarget,
            ("stays", _) => TableChoice::Stays,
            _ => continue,
        };
        overrides.tables.insert(c.source_table, choice);
    }
    for c in file.column_choices {
        overrides
            .columns
            .insert((c.source_table, c.source_column), c.target_column);
    }
    Ok(Workspace {
        source,
        target,
        using_samples: file.using_samples,
        word_list: file.word_list,
        overrides,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Side;

    #[test]
    fn a_saved_workspace_opens_with_everything_in_it() {
        let mut ws = Workspace::new();
        ws.load_samples();
        ws.source.name = "Workshop \"main\"".into();
        ws.word_list.push_str("\nlogin = orcid\n");
        ws.set_table_choice(
            "main.booking",
            Some(TableChoice::Target("public.session".into())),
        );
        ws.set_table_choice("main.operator", Some(TableChoice::Stays));
        ws.set_column_choice("main.machine", "room", Some(Some("location_id".into())));
        ws.set_column_choice("main.machine", "notes", Some(None));

        let opened = open_workspace(&save_workspace(&ws)).expect("opens");
        assert_eq!(opened.source.name, ws.source.name);
        assert_eq!(opened.source.input, ws.source.input);
        assert_eq!(opened.source.schema, ws.source.schema);
        assert_eq!(opened.target.schema, ws.target.schema);
        assert_eq!(opened.word_list, ws.word_list);
        assert_eq!(opened.overrides, ws.overrides);
        assert!(opened.using_samples);
        // The analysis is the same, so the plan and SQL are too.
        assert_eq!(opened.analysis(), ws.analysis());
    }

    #[test]
    fn a_workspace_with_one_database_and_the_four_file_export_opens() {
        let mut ws = Workspace::new();
        for part in ["columns", "constraints", "indexes", "rowcounts"] {
            let path = format!(
                "{}/../../demo/catalogue-postgresql-4files/{part}.csv",
                env!("CARGO_MANIFEST_DIR")
            );
            ws.add_file(Side::Target, part, std::fs::read_to_string(path).unwrap());
        }
        let opened = open_workspace(&save_workspace(&ws)).unwrap();
        assert!(opened.source.schema.is_none());
        assert_eq!(opened.target.input, ws.target.input);
        assert_eq!(opened.target.schema.as_ref().unwrap().tables.len(), 7);
    }

    #[test]
    fn other_files_are_rejected_with_a_reason() {
        assert_eq!(
            open_workspace("item,price").unwrap_err(),
            OpenError::NotJson
        );
        assert_eq!(
            open_workspace(r#"{"name":"x"}"#).unwrap_err(),
            OpenError::NotAWorkspace
        );
        let newer = r#"{"format":"schema-bridge-workspace","version":99}"#;
        assert_eq!(
            open_workspace(newer).unwrap_err(),
            OpenError::NewerVersion(99)
        );
        assert!(
            OpenError::NewerVersion(99)
                .to_string()
                .contains("newer version")
        );
    }

    #[test]
    fn version_2_records_the_importer_of_each_database() {
        let mut ws = Workspace::new();
        ws.load_samples();
        ws.clear(Side::Target);
        let text = save_workspace(&ws);
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["version"], 2);
        assert_eq!(v["source"]["importer"], "csv-export");
        assert!(
            v["source"]["input"]["combined"]
                .as_str()
                .unwrap()
                .starts_with("record_type,")
        );
        // An empty database records no importer at all.
        assert!(v["target"].get("importer").is_none() && v["target"].get("input").is_none());

        let opened = open_workspace(&text).unwrap();
        assert_eq!(opened.source.input, ws.source.input);
        assert_eq!(opened.source.schema, ws.source.schema);
        assert_eq!(opened.target.input, SchemaInput::Empty);
        assert!(opened.target.schema.is_none());
    }

    #[test]
    fn a_version_1_file_still_opens_with_the_same_result() {
        let mut ws = Workspace::new();
        ws.load_samples();
        ws.set_table_choice(
            "main.booking",
            Some(TableChoice::Target("public.session".into())),
        );
        ws.set_column_choice("main.machine", "room", Some(Some("location_id".into())));
        let csv = |slot: &Slot| slot.input.csv_files().unwrap().combined.clone().unwrap();
        // Exactly the layout format version 1 wrote.
        let v1 = json!({
            "format": "schema-bridge-workspace",
            "version": 1,
            "source": { "name": ws.source.name, "combined": csv(&ws.source) },
            "target": { "name": ws.target.name, "combined": csv(&ws.target) },
            "using_samples": true,
            "word_list": ws.word_list,
            "table_choices": [{ "source_table": "main.booking", "choice": "target", "target_table": "public.session" }],
            "column_choices": [{ "source_table": "main.machine", "source_column": "room", "target_column": "location_id" }]
        })
        .to_string();
        let opened = open_workspace(&v1).expect("version 1 opens");
        assert_eq!(opened.source.input, ws.source.input);
        assert_eq!(opened.target.input, ws.target.input);
        assert_eq!(opened.overrides, ws.overrides);
        assert_eq!(opened.analysis(), ws.analysis());
        // Saving it again writes version 2.
        assert!(save_workspace(&opened).contains("\"version\": 2"));

        // Version 1 four-file slots and empty slots open too.
        let four = json!({
            "format": "schema-bridge-workspace", "version": 1,
            "source": { "name": "" },
            "target": { "name": "Catalogue", "columns": "table_schema,table_name,object_type,ordinal_position,column_name,data_type,is_nullable,column_default,table_comment,column_comment\npublic,asset,table,1,uuid,uuid,false,,,\n" }
        });
        let opened = open_workspace(&four.to_string()).unwrap();
        assert_eq!(opened.source.name, "Source");
        assert_eq!(opened.source.input, SchemaInput::Empty);
        assert_eq!(opened.target.schema.as_ref().unwrap().tables.len(), 1);
    }

    #[test]
    fn an_unknown_importer_is_refused_with_a_clear_message() {
        let file = json!({
            "format": "schema-bridge-workspace", "version": 2,
            "source": { "name": "Telemetry", "importer": "influxdb-schema", "input": {} },
            "target": { "name": "Target" }
        });
        let error = open_workspace(&file.to_string()).unwrap_err();
        assert_eq!(error, OpenError::UnknownImporter("influxdb-schema".into()));
        let message = error.to_string();
        assert!(message.contains("\"influxdb-schema\""));
        assert!(message.contains("does not support"));
        assert!(message.contains("Update Schema Bridge"));
    }

    #[test]
    fn malformed_slots_are_refused() {
        let base = |source: Value| {
            json!({ "format": "schema-bridge-workspace", "version": 2, "source": source, "target": { "name": "T" } }).to_string()
        };
        for bad in [
            json!("not an object"),
            json!({ "name": 3 }),
            json!({ "name": "S", "importer": 7 }),
            json!({ "name": "S", "importer": "csv-export", "input": { "combined": false } }),
        ] {
            assert_eq!(
                open_workspace(&base(bad.clone())).unwrap_err(),
                OpenError::NotAWorkspace,
                "{bad}"
            );
        }
    }
}
