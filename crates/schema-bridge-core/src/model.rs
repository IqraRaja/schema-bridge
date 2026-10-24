//! Database-neutral schema model.
//!
//! Field names follow the prototype's `buildModel` so that both stay easy to
//! compare. Every import route produces this same model.

use std::collections::BTreeSet;

/// The structure of one database, as read from its export.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Schema {
    /// Database system, lower case: `postgresql`, `mysql`, `sqlserver`,
    /// `oracle` or `sqlite`. Defaults to `postgresql`, as in the prototype.
    pub dbms: String,
    /// Tables (not views), sorted by name.
    pub tables: Vec<Table>,
    /// Keys (`schema.name`) of views and materialised views.
    pub views: BTreeSet<String>,
    /// Which parts of the export were provided.
    pub has: Provided,
    /// Retention policies of a time-series database (how long data is
    /// kept). Empty for relational databases.
    pub retention_policies: Vec<RetentionPolicy>,
}

/// What kind of object a [`Table`] describes. Relational databases have
/// tables; time-series databases describe measurements (series of points
/// with a timestamp, tags and fields).
///
/// This is an extension point: no importer produces measurements yet, and
/// checks and matching treat every table alike (see docs/architecture.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TableKind {
    #[default]
    Table,
    Measurement,
}

/// The role of a column in a time-series measurement. Relational columns
/// are `Regular`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColumnRole {
    #[default]
    Regular,
    /// The point's time.
    Timestamp,
    /// An indexed label that identifies a series (InfluxDB tag, Prometheus label).
    Tag,
    /// A measured value.
    Field,
}

/// How long a time-series database keeps data.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RetentionPolicy {
    pub name: String,
    /// As the database writes it, for example `30d` or `INF`.
    pub duration: Option<String>,
    pub default: bool,
}

/// Which parts of an export were present. Checks that need a missing part
/// are skipped later instead of reporting false problems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Provided {
    pub columns: bool,
    pub constraints: bool,
    pub indexes: bool,
    pub row_counts: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    /// `schema.name`, used to refer to the table everywhere else.
    pub key: String,
    pub schema: String,
    pub name: String,
    /// Columns in their database order.
    pub columns: Vec<Column>,
    pub comment: Option<String>,
    /// Estimated row count from the database statistics, if exported.
    /// PostgreSQL reports -1 for tables that were never analysed.
    pub rows: Option<i64>,
    /// Names of user triggers, comma-separated, as exported.
    pub triggers: String,
    /// Primary key columns; empty when the table has no primary key.
    pub primary_key: Vec<String>,
    pub primary_key_name: Option<String>,
    pub uniques: Vec<UniqueKey>,
    pub foreign_keys: Vec<ForeignKey>,
    pub checks: Vec<Check>,
    pub indexes: Vec<Index>,
    pub kind: TableKind,
    /// The retention policy a measurement belongs to, by name.
    pub retention_policy: Option<String>,
    /// Number of series (tag combinations) of a measurement, if exported.
    /// Not a row count: a series holds any number of points.
    pub series: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Column {
    pub name: String,
    /// The type exactly as the database spells it, shown to users.
    pub type_label: String,
    /// The type in lower case, normalised for comparisons
    /// (for example Oracle `DATE` becomes `timestamp(0) without time zone`).
    pub data_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub comment: Option<String>,
    /// Position of the column in its table, starting at 1.
    pub position: i64,
    pub role: ColumnRole,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct UniqueKey {
    pub name: Option<String>,
    pub columns: Vec<String>,
}

/// A link (foreign key) from columns of one table to another table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ForeignKey {
    pub name: Option<String>,
    pub columns: Vec<String>,
    /// Key (`schema.name`) of the referenced table.
    pub references: String,
    pub referenced_columns: Vec<String>,
    /// True when the link was never checked for existing rows
    /// (`NOT VALID`, untrusted or disabled).
    pub not_valid: bool,
    /// `CASCADE`, `SET NULL`, `SET DEFAULT` or `RESTRICT`; `None` means no action.
    pub on_delete: Option<String>,
    /// The constraint definition as exported.
    pub definition: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Check {
    pub name: Option<String>,
    pub definition: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Index {
    pub name: Option<String>,
    /// The index definition as exported.
    pub definition: String,
    pub unique: bool,
    /// True for an index with a `WHERE` condition.
    pub partial: bool,
    /// Indexed columns or expressions, in order.
    pub columns: Vec<String>,
}

/// Counts shown when two schemas are compared side by side
/// (the prototype's "Side by side" card).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SchemaStats {
    pub tables: usize,
    pub columns: usize,
    /// Number of foreign keys.
    pub links: usize,
    pub tables_with_primary_key: usize,
    pub columns_with_description: usize,
    /// Sum of the positive row estimates; unknown estimates count as 0.
    pub estimated_rows: i64,
}

/// Counts for a time-series schema: measurements, tags, fields and the sum
/// of the exported series counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TimeSeriesStats {
    pub measurements: usize,
    pub tags: usize,
    pub fields: usize,
    /// Sum of the exported series counts; `None` when none was exported.
    pub series: Option<i64>,
}

impl Schema {
    /// Measurement, tag, field and series counts (all zero for a relational
    /// schema).
    pub fn time_series_stats(&self) -> TimeSeriesStats {
        let m: Vec<&Table> = self
            .tables
            .iter()
            .filter(|t| t.kind == TableKind::Measurement)
            .collect();
        let count = |role: ColumnRole| {
            m.iter()
                .flat_map(|t| &t.columns)
                .filter(|c| c.role == role)
                .count()
        };
        let series: Vec<i64> = m.iter().filter_map(|t| t.series).collect();
        TimeSeriesStats {
            measurements: m.len(),
            tags: count(ColumnRole::Tag),
            fields: count(ColumnRole::Field),
            series: (!series.is_empty()).then(|| series.iter().sum()),
        }
    }

    /// True when the schema describes any time-series measurement.
    pub fn is_time_series(&self) -> bool {
        self.tables.iter().any(|t| t.kind == TableKind::Measurement)
    }

    /// What the schema's objects are called: "measurements" or "tables".
    pub fn object_noun(&self) -> &'static str {
        if self.is_time_series() {
            "measurements"
        } else {
            "tables"
        }
    }

    /// Finds a table by its key (`schema.name`).
    pub fn table(&self, key: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.key == key)
    }

    pub fn stats(&self) -> SchemaStats {
        let t = &self.tables;
        SchemaStats {
            tables: t.len(),
            columns: t.iter().map(|t| t.columns.len()).sum(),
            links: t.iter().map(|t| t.foreign_keys.len()).sum(),
            tables_with_primary_key: t.iter().filter(|t| !t.primary_key.is_empty()).count(),
            columns_with_description: t
                .iter()
                .flat_map(|t| &t.columns)
                .filter(|c| c.comment.as_deref().is_some_and(|s| !s.is_empty()))
                .count(),
            estimated_rows: t.iter().filter_map(|t| t.rows).filter(|r| *r > 0).sum(),
        }
    }
}

impl Table {
    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == name)
    }

    /// The link (foreign key) made of this one column alone, if any.
    pub fn link_of(&self, column: &str) -> Option<&ForeignKey> {
        self.foreign_keys
            .iter()
            .find(|f| f.columns.len() == 1 && f.columns[0] == column)
    }
}

/// The table part of a key: `public.asset` gives `asset`.
pub fn table_name(key: &str) -> &str {
    key.split_once('.').map_or("", |(_, name)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_name_drops_the_schema_only() {
        assert_eq!(table_name("public.asset"), "asset");
        assert_eq!(table_name("main.a.b"), "a.b");
        assert_eq!(table_name("noschema"), "");
    }

    #[test]
    fn stats_count_tables_columns_links_and_positive_row_estimates() {
        let column = |comment: Option<&str>| Column {
            comment: comment.map(str::to_string),
            ..Default::default()
        };
        let schema = Schema {
            tables: vec![
                Table {
                    columns: vec![column(Some("id")), column(Some("")), column(None)],
                    primary_key: vec!["id".into()],
                    foreign_keys: vec![ForeignKey::default(), ForeignKey::default()],
                    rows: Some(120),
                    ..Default::default()
                },
                Table {
                    columns: vec![column(None)],
                    rows: Some(-1), // never analysed: counts as unknown
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            schema.stats(),
            SchemaStats {
                tables: 2,
                columns: 4,
                links: 2,
                tables_with_primary_key: 1,
                columns_with_description: 1,
                estimated_rows: 120,
            }
        );
    }

    #[test]
    fn imported_schemas_are_relational_and_measurements_are_recognised() {
        let mut ws = crate::workspace::Workspace::new();
        ws.load_samples();
        let source = ws.source.schema.as_ref().unwrap();
        assert!(!source.is_time_series());
        assert!(source.tables.iter().all(|t| t.kind == TableKind::Table));
        assert!(
            source
                .tables
                .iter()
                .flat_map(|t| &t.columns)
                .all(|c| c.role == ColumnRole::Regular)
        );
        assert!(source.retention_policies.is_empty());

        // A hand-built measurement keeps its time-series facts, and the
        // neutral engine still analyses it without failing.
        let role = |name: &str, role: ColumnRole, ty: &str| Column {
            name: name.into(),
            type_label: ty.into(),
            data_type: ty.into(),
            nullable: role != ColumnRole::Timestamp,
            role,
            ..Default::default()
        };
        let cpu = Table {
            key: "telemetry.cpu".into(),
            schema: "telemetry".into(),
            name: "cpu".into(),
            kind: TableKind::Measurement,
            retention_policy: Some("thirty_days".into()),
            columns: vec![
                role("time", ColumnRole::Timestamp, "timestamp with time zone"),
                role("host", ColumnRole::Tag, "text"),
                role("usage", ColumnRole::Field, "double precision"),
            ],
            ..Default::default()
        };
        let schema = Schema {
            dbms: "influxdb".into(),
            tables: vec![cpu],
            retention_policies: vec![RetentionPolicy {
                name: "thirty_days".into(),
                duration: Some("30d".into()),
                default: true,
            }],
            ..Default::default()
        };
        assert!(schema.is_time_series());
        let mut ws = crate::workspace::Workspace::new();
        ws.source.schema = Some(schema.clone());
        ws.target.schema = Some(schema);
        let a = ws.analysis().expect("analysed");
        assert_eq!(a.matching.mappings.len(), 1);
        assert_eq!(a.matching.mappings[0].rows[1].source.role, ColumnRole::Tag);
    }
}
