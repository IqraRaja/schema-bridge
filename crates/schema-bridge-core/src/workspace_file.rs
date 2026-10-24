//! Saving and reopening a workspace (FR-04): both structure exports as they
//! were loaded, the database names, the word list and every choice made by
//! hand. The file is JSON and stays on the user's computer; it holds
//! structure only (no table contents) and never a password.

use serde::{Deserialize, Serialize};

use crate::import::{ExportFiles, build_schema};
use crate::matching::{Overrides, TableChoice};
use crate::workspace::{Slot, Workspace};

/// Written into every file, so other JSON files are recognised.
const FORMAT: &str = "schema-bridge-workspace";
/// Raised when the file layout changes in a way older versions cannot read.
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct File {
    format: String,
    version: u32,
    source: SlotFile,
    target: SlotFile,
    #[serde(default)]
    using_samples: bool,
    #[serde(default)]
    word_list: String,
    #[serde(default)]
    table_choices: Vec<TableChoiceFile>,
    #[serde(default)]
    column_choices: Vec<ColumnChoiceFile>,
}

#[derive(Serialize, Deserialize, Default)]
struct SlotFile {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    combined: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    columns: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    constraints: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    indexes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    row_counts: Option<String>,
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

/// Why a workspace file could not be opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    NotJson,
    NotAWorkspace,
    NewerVersion(u32),
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
        }
    }
}

impl std::error::Error for OpenError {}

fn slot_file(slot: &Slot) -> SlotFile {
    let f = &slot.files;
    SlotFile {
        name: slot.name.clone(),
        combined: f.combined.clone(),
        columns: f.columns.clone(),
        constraints: f.constraints.clone(),
        indexes: f.indexes.clone(),
        row_counts: f.row_counts.clone(),
    }
}

fn slot_from(file: SlotFile, fallback: &str) -> Slot {
    let files = ExportFiles {
        combined: file.combined,
        columns: file.columns,
        constraints: file.constraints,
        indexes: file.indexes,
        row_counts: file.row_counts,
    };
    let schema = (!files.is_empty()).then(|| build_schema(&files));
    Slot {
        name: if file.name.trim().is_empty() {
            fallback.to_string()
        } else {
            file.name
        },
        files,
        schema,
        notice: None,
    }
}

/// Writes a workspace as pretty-printed JSON.
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

/// Reads a workspace saved with [`save_workspace`]. Choices of an unknown
/// kind are skipped; choices for tables that no longer exist are kept and
/// simply have no effect, as in the running application.
pub fn open_workspace(text: &str) -> Result<Workspace, OpenError> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| OpenError::NotJson)?;
    if value.get("format").and_then(|f| f.as_str()) != Some(FORMAT) {
        return Err(OpenError::NotAWorkspace);
    }
    let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    if version > VERSION {
        return Err(OpenError::NewerVersion(version));
    }
    let file: File = serde_json::from_value(value).map_err(|_| OpenError::NotAWorkspace)?;

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
        source: slot_from(file.source, "Source"),
        target: slot_from(file.target, "Target"),
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
        assert_eq!(opened.source.files, ws.source.files);
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
        assert_eq!(opened.target.files, ws.target.files);
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
}
