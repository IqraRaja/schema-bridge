# Adding a database system

A guide for developers who want Schema Bridge to understand another
database system. Read [architecture.md](architecture.md) first.

There are two cases:

1. **The system can produce the one-file export** (a relational database
   whose catalogue can be queried for tables, columns, keys and indexes).
   This needs no change to the engine; follow the steps below.
2. **The system's structure does not fit the one-file export** (for example
   a time-series database with measurements, tags and fields). This needs a
   new importer; the model already holds time-series concepts, and InfluxDB
   is the worked example. See the end of this page.

## Case 1: a relational system

Example: adding IBM Db2 with the code `db2`.

### 1. Write the export query

Add `export-queries/db2.sql`. It must:

- only **read** the catalogue (no `INSERT`, `UPDATE`, `CREATE`, …; a test
  checks this), and never read table contents;
- return the 21 columns of the one-file export in this order:
  `record_type, table_schema, table_name, object_type, ordinal_position,
  column_name, data_type, is_nullable, column_default, table_comment,
  column_comment, constraint_name, constraint_type, definition, ref_schema,
  ref_table, indexname, indexdef, estimated_rows, triggers, dbms`;
- write `db2` in the `dbms` column of every row.

Use `export-queries/postgresql.sql` as the reference for what each
`record_type` (`column`, `constraint`, `index`, `table`) row contains, and
for the `definition` / `indexdef` text the importer reads
(`PRIMARY KEY (…)`, `FOREIGN KEY (…) REFERENCES schema.table(…)`,
`CREATE [UNIQUE] INDEX … ON … (…)`).

### 2. Register the system

Add an entry to `SYSTEMS` in `crates/schema-bridge-core/src/systems.rs`:

```rust
DatabaseSystem {
    code: "db2",
    name: "IBM Db2",
    type_overrides: &[],          // names that mean something else in Db2
    validate_link: None,          // or Some(|schema, table, name| format!(…))
    description_fix: Some("Add COMMENT ON COLUMN for the important columns."),
    foreign_data_wrapper: None,   // no wrapper: the draft uses CSV staging
    export: Some(ExportQuery {
        steps: ["Where to run it…", "How to run it…", "How to save as CSV…"],
        query: include_str!("../../../export-queries/db2.sql"),
        tested: false,            // true only after running it on a real server
    }),
},
```

The Load stage then lists Db2 under "How to make an export file", and
findings, matching, the plan and the draft SQL work for it.

### 3. Teach the type names

Run the query on a test database and collect the `data_type` values. Any
name that `types.rs` does not recognise is treated as `Other`, which makes
type comparisons weak. Add missing names to the shared vocabulary in
`type_info` (`crates/schema-bridge-core/src/types.rs`) and add a line per
name to `tests/fixtures/types.input.txt` with the expected answer in
`types.expected.txt`. If a name means something different in this system
than elsewhere (like Oracle's `DATE`), put it in `type_overrides` instead.

### 4. Test

- Make a small made-up database on the system, export it with the query and
  add an import test like those in `tests/import_parity.rs`.
- Run `cargo fmt`, `cargo check` and `cargo test`.
- Never commit a real organisation's schema (decision 7).

### 5. Document

Update the "Database systems" table in the README: the system moves from
"planned" to "recognised and importable", with "tested on a real
database: no" until it has been.

## Case 2: a system that does not fit the one-file export

Time-series databases such as InfluxDB describe their structure as
measurements with tags, fields, a timestamp and retention policies, not as
tables with keys and links. Forcing them into tables would lose meaning
(a tag is not just a text column; a retention policy is not a column at
all).

**Already there:** the model's extension points. A measurement is a `Table`
with `kind: TableKind::Measurement`; its columns carry a `role`
(`ColumnRole::Timestamp`, `Tag` or `Field`); `Table::retention_policy` names
its policy and `Schema::retention_policies` lists them
(`crates/schema-bridge-core/src/model.rs`). A test checks that such a schema
passes through the analysis.

**Also already there:** the import boundary (`crates/schema-bridge-core/src/input.rs`).
A new format adds a `SchemaInput` variant and an `Importer` with a new,
never-changing id; the slot, the saved comparison (which records the
importer id) and the application need no change.

**Worked example: InfluxDB (milestone 8, done).** Follow it for the next
time-series system (Prometheus, OpenTelemetry metrics, another time-series
database):

1. A general input in the workspace slot and the workspace file (milestone 7).
2. An interchange format and importer: `SchemaInput::InfluxSchema`,
   `InfluxSchemaImporter` (id `influx-schema-csv`) and the parser in
   `influx.rs`, with line-numbered errors. Put it in `importers()` before any
   importer that would also accept its files, and test that each importer
   claims only its own files (`input.rs`).
3. A registry entry (`systems.rs`): `influxdb` with type overrides for the
   field types, no foreign data wrapper, and the export recipes in
   `export-queries/` (marked untested until run on a real server).
4. Checks and rules that read the time-series facts, isolated from the
   relational ones: `TS-*` in `checks.rs`, `MAP-TS-*` in `integration.rs`,
   precision, unknown types and role changes in `matching.rs`.
5. Plan and draft output: CSV staging for the source; no SQL for a
   time-series target (`sql::draft_available`).
6. A made-up sample in `demo/` with its word list, the sample loader in
   `workspace.rs`, and an end-to-end test (`tests/timeseries.rs`).
7. Documentation: [time series](time-series.md), and the overview pages.

Until these exist, Schema Bridge cannot import or meaningfully compare
time-series schemas, and the documentation must say so.
