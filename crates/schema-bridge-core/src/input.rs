//! What a database slot was loaded from, and the importers that turn it into
//! the neutral model. This is the boundary for new input formats.
//!
//! A slot holds a [`SchemaInput`]: one variant per input format, so a format
//! keeps exactly the files it needs. An [`Importer`] recognises its files,
//! adds them to an input, builds the neutral [`Schema`], and writes and reads
//! its input in a saved comparison. The rest of the library only sees the
//! `Schema`, so adding a format means adding a variant and an importer
//! (see docs/adding-a-database.md).

use serde_json::{Value, json};

use crate::import::{ExportFiles, ImportError, build_schema};
use crate::model::Schema;
use crate::workspace::kind_label;

/// The files loaded for one database, by input format.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SchemaInput {
    /// Nothing loaded yet.
    #[default]
    Empty,
    /// Structure exports in CSV: the one-file export, or the parts of the
    /// older four-file PostgreSQL export.
    CsvExport(ExportFiles),
}

impl SchemaInput {
    pub fn is_empty(&self) -> bool {
        match self {
            SchemaInput::Empty => true,
            SchemaInput::CsvExport(files) => files.is_empty(),
        }
    }

    /// The importer that reads this input, or `None` when it is empty.
    pub fn importer(&self) -> Option<&'static dyn Importer> {
        match self {
            SchemaInput::Empty => None,
            SchemaInput::CsvExport(_) => Some(&CSV_EXPORT),
        }
    }

    /// The CSV export files, when the input is a CSV export.
    pub fn csv_files(&self) -> Option<&ExportFiles> {
        match self {
            SchemaInput::CsvExport(files) => Some(files),
            SchemaInput::Empty => None,
        }
    }

    /// The neutral model, built by this input's importer.
    pub fn build(&self) -> Option<Schema> {
        self.importer().and_then(|i| i.build(self))
    }
}

/// One input format: how its files are recognised, collected, turned into
/// the neutral model, and kept in a saved comparison.
pub trait Importer: Sync {
    /// Stable id written into saved comparisons. Never change it once
    /// released: files saved with it must keep opening.
    fn id(&self) -> &'static str;
    /// Name shown to users.
    fn name(&self) -> &'static str;
    /// Whether a file belongs to this format, judged from its contents.
    fn recognises(&self, text: &str) -> bool;
    /// Adds one file to an input of this format; an input of another format
    /// is replaced. Returns a short description of what the file was. On an
    /// error the input is left unchanged.
    fn add_file(&self, input: &mut SchemaInput, text: String) -> Result<String, ImportError>;
    /// Builds the neutral model; `None` when the input is not this format or
    /// is empty.
    fn build(&self, input: &SchemaInput) -> Option<Schema>;
    /// The input as JSON for a saved comparison.
    fn save(&self, input: &SchemaInput) -> Value;
    /// Reads the input back from a saved comparison; `None` when the JSON
    /// does not describe a valid input of this format.
    fn open(&self, value: &Value) -> Option<SchemaInput>;
}

/// Every importer, in the order they are asked to recognise a file.
pub fn importers() -> [&'static dyn Importer; 1] {
    [&CSV_EXPORT]
}

/// The importer with this id.
pub fn importer(id: &str) -> Option<&'static dyn Importer> {
    importers().into_iter().find(|i| i.id() == id)
}

/// The first importer that recognises a file.
pub fn importer_for(text: &str) -> Option<&'static dyn Importer> {
    importers().into_iter().find(|i| i.recognises(text))
}

/// The structure exports in CSV (see docs/import-formats.md). It only
/// delegates to `import.rs`, so CSV import behaves exactly as before.
pub struct CsvExportImporter;

pub static CSV_EXPORT: CsvExportImporter = CsvExportImporter;

/// The fields of the CSV export parts in a saved comparison.
const CSV_PARTS: [&str; 5] = [
    "combined",
    "columns",
    "constraints",
    "indexes",
    "row_counts",
];

impl Importer for CsvExportImporter {
    fn id(&self) -> &'static str {
        "csv-export"
    }

    fn name(&self) -> &'static str {
        "Structure export (CSV)"
    }

    fn recognises(&self, text: &str) -> bool {
        crate::import::detect_kind(text).is_some()
    }

    fn add_file(&self, input: &mut SchemaInput, text: String) -> Result<String, ImportError> {
        let mut files = input.csv_files().cloned().unwrap_or_default();
        let kind = files.add(text)?;
        *input = SchemaInput::CsvExport(files);
        Ok(kind_label(kind).to_string())
    }

    fn build(&self, input: &SchemaInput) -> Option<Schema> {
        input
            .csv_files()
            .filter(|f| !f.is_empty())
            .map(build_schema)
    }

    fn save(&self, input: &SchemaInput) -> Value {
        let mut out = serde_json::Map::new();
        if let Some(f) = input.csv_files() {
            let parts = [
                &f.combined,
                &f.columns,
                &f.constraints,
                &f.indexes,
                &f.row_counts,
            ];
            for (name, part) in CSV_PARTS.iter().zip(parts) {
                if let Some(text) = part {
                    out.insert(name.to_string(), json!(text));
                }
            }
        }
        Value::Object(out)
    }

    fn open(&self, value: &Value) -> Option<SchemaInput> {
        let object = value.as_object()?;
        let mut parts: [Option<String>; 5] = Default::default();
        for (i, name) in CSV_PARTS.iter().enumerate() {
            match object.get(*name) {
                None | Some(Value::Null) => {}
                Some(Value::String(text)) => parts[i] = Some(text.clone()),
                Some(_) => return None,
            }
        }
        let [combined, columns, constraints, indexes, row_counts] = parts;
        let files = ExportFiles {
            combined,
            columns,
            constraints,
            indexes,
            row_counts,
        };
        Some(if files.is_empty() {
            SchemaInput::Empty
        } else {
            SchemaInput::CsvExport(files)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo(path: &str) -> String {
        std::fs::read_to_string(format!("{}/../../demo/{path}", env!("CARGO_MANIFEST_DIR")))
            .unwrap()
    }

    /// Adds files the old way (`ExportFiles` + `build_schema`) and through
    /// the importer, and compares the results.
    fn same_both_ways(paths: &[&str]) {
        let mut files = ExportFiles::default();
        let mut input = SchemaInput::Empty;
        for p in paths {
            let text = demo(p);
            let before = files.add(text.clone()).map(kind_label).map(str::to_string);
            let after = importer_for(&text)
                .expect("recognised")
                .add_file(&mut input, text);
            assert_eq!(after.as_deref().ok(), before.as_deref().ok(), "{p}");
        }
        assert_eq!(input, SchemaInput::CsvExport(files.clone()));
        assert_eq!(input.build(), Some(build_schema(&files)), "{paths:?}");
        assert_eq!(input.importer().unwrap().id(), "csv-export");
    }

    #[test]
    fn the_csv_importer_builds_the_same_schema_as_before() {
        same_both_ways(&["workshop-sqlite.csv"]);
        same_both_ways(&["catalogue-postgresql.csv"]);
        same_both_ways(&[
            "catalogue-postgresql-4files/columns.csv",
            "catalogue-postgresql-4files/constraints.csv",
            "catalogue-postgresql-4files/indexes.csv",
            "catalogue-postgresql-4files/rowcounts.csv",
        ]);
        // A one-file export after parts replaces them, exactly as before.
        same_both_ways(&[
            "catalogue-postgresql-4files/columns.csv",
            "workshop-sqlite.csv",
        ]);
    }

    #[test]
    fn files_that_are_no_export_are_recognised_by_no_importer() {
        assert!(importer_for("item,price\nbread,2\n").is_none());
        assert!(importer_for("").is_none());
        let mut input = SchemaInput::Empty;
        assert!(
            CSV_EXPORT
                .add_file(&mut input, "item,price\n".into())
                .is_err()
        );
        assert_eq!(input, SchemaInput::Empty, "a refused file changes nothing");
    }

    #[test]
    fn importers_are_found_by_their_stable_id() {
        assert_eq!(
            importer("csv-export").unwrap().name(),
            "Structure export (CSV)"
        );
        assert!(importer("influxdb").is_none());
        assert!(SchemaInput::Empty.importer().is_none() && SchemaInput::Empty.build().is_none());
        assert!(SchemaInput::Empty.is_empty());
    }

    #[test]
    fn the_csv_input_survives_saving_and_opening() {
        let mut input = SchemaInput::Empty;
        for p in [
            "catalogue-postgresql-4files/columns.csv",
            "catalogue-postgresql-4files/indexes.csv",
        ] {
            CSV_EXPORT.add_file(&mut input, demo(p)).unwrap();
        }
        let saved = CSV_EXPORT.save(&input);
        assert_eq!(
            saved.as_object().unwrap().len(),
            2,
            "only the parts present are written"
        );
        assert_eq!(CSV_EXPORT.open(&saved), Some(input));
        assert_eq!(CSV_EXPORT.open(&json!({})), Some(SchemaInput::Empty));
        assert_eq!(
            CSV_EXPORT.open(&json!({"combined": 7})),
            None,
            "a wrong type is refused"
        );
        assert_eq!(CSV_EXPORT.open(&json!("text")), None);
    }
}
