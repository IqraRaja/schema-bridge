//! Core library of Schema Bridge.
//!
//! It contains the database-neutral schema model, the import of the structure
//! exports that the prototype supports, type recognition (`types`), problem
//! detection inside each database (`checks`), table and column matching
//! (`matching`), integration findings, load order and draft SQL
//! (`integration`, `plan`, `sql`) and the state of the two databases being
//! compared (`workspace`), saved and reopened with `workspace_file`. Exports
//! for a meeting or a ticket are in `export`. Everything specific to one
//! database system is in `systems`, the extension point for new adapters.
//!
//! ```
//! use schema_bridge_core::{ExportFiles, build_schema};
//!
//! let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../demo/workshop-sqlite.csv");
//! let mut files = ExportFiles::default();
//! files.add(std::fs::read_to_string(path).unwrap()).unwrap();
//! let workshop = build_schema(&files);
//!
//! assert_eq!(workshop.dbms, "sqlite");
//! assert_eq!(workshop.tables.len(), 5);
//! assert!(workshop.table("main.run").is_some());
//! ```

pub mod checks;
mod csv_reader;
pub mod export;
pub mod import;
pub mod integration;
pub mod matching;
pub mod model;
pub mod plan;
pub mod report;
pub mod sql;
pub mod systems;
pub mod types;
pub mod workspace;
pub mod workspace_file;

pub use checks::{Finding, Severity, check_schema, count_by_severity};
pub use import::{ExportFiles, ExportKind, ImportError, build_schema, detect_kind};
pub use matching::{
    ColumnStatus, Matching, Overrides, TableChoice, TableMapping, TableMatch, WordList,
    match_schemas,
};
pub use model::{
    Check, Column, ColumnRole, ForeignKey, Index, Provided, RetentionPolicy, Schema, SchemaStats,
    Table, TableKind, UniqueKey, table_name,
};
pub use types::{Category, Compatibility, Fit, TypeInfo, UNLIMITED, compatibility, type_info};
