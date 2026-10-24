# Adding a database system

A guide for developers who want Schema Bridge to understand another
database system. Read [architecture.md](architecture.md) first.

There are two cases:

1. **The system can produce the one-file export** (a relational database
   whose catalogue can be queried for tables, columns, keys and indexes).
   This needs no change to the engine; follow the steps below.
2. **The system's structure does not fit the one-file export** (for example
   a time-series database with measurements, tags and fields). This needs a
   new importer and, for time-series concepts, model support that is not
   built yet. See the end of this page.

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

**Still needed**, in this order:

1. A general input in the workspace slot and the workspace file, so a format
   other than the CSV export can be stored (development plan, milestone 7).
2. An importer for the system's schema description (for InfluxDB, for
   example the output of `SHOW MEASUREMENTS`, `SHOW TAG KEYS`,
   `SHOW FIELD KEYS` and `SHOW RETENTION POLICIES`) that fills the model,
   with a made-up sample and tests.
3. A registry entry as in case 1 (type names such as `float`, `integer`,
   `string`, `boolean` for fields).
4. Checks and matching rules that read the time-series facts: for example
   tags matched to tags, a measurement without a retention policy, a field
   whose type differs between source and target, and how a relational table
   maps to a measurement (which columns become tags).
5. Plan and draft output suitable for a time-series target.

Until these exist, Schema Bridge cannot import or meaningfully compare
time-series schemas, and the documentation must say so.
