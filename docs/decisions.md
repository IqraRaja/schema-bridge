# Decisions and assumptions

Important choices made during development, and why. Newest last. When a
decision changes, update its entry rather than deleting it.

The requirements themselves are in [specification.md](specification.md).
Where a decision here makes the implementation differ from the specification,
the difference is also listed at the end of the specification.

### 1. The browser prototype is the reference

Schema Bridge started as a single-file browser prototype. The Rust
application reproduces its workflow, terms and behaviour; changes are made only
for a clear technical reason and are recorded here. Behaviour is ported module
by module and checked against the prototype with parity tests (decision 6).

### 2. Rust core library first, Dioxus application on top

The analysis logic lives in a plain Rust library (`schema-bridge-core`) with no
UI code, so it can be tested without a window or a database. The Dioxus
application is a second crate on top (`schema-bridge-app`; first built as a
web app, see decision 13). More crates are added only when there is a real
need. *Updated 23 October 2026: originally "Dioxus desktop app later".*

### 3. Release 1 reads export files, not live databases

The prototype works from structure exports (CSV) produced by one read-only
query per database system. Release 1 keeps this: users run the query in their
usual tool (pgAdmin, MySQL Workbench, SSMS, SQL Developer, DB Browser for
SQLite) and load the file. This keeps credentials out of the application
entirely. Direct database connections may come later.

### 4. Two export formats

- **One-file export** (current): 21 columns, the `record_type` column says
  whether a row describes a column, constraint, index or table. Queries for
  five systems are in `export-queries/`. Files saved without a header row and
  files using `;` as separator are accepted.
- **Four-file export** (older, PostgreSQL only): columns, constraints, indexes
  and row counts in separate files. Kept because existing exports use it.

The PostgreSQL and SQLite queries were run on real databases. The MySQL/MariaDB,
SQL Server and Oracle queries were reviewed but not yet run on real servers.

### 5. Own CSV reader instead of a CSV crate

The prototype distinguishes an unquoted `NULL` or empty field (no value) from a
quoted `"NULL"` or `""` (text). The `csv` crate does not expose whether a field
was quoted, so the prototype's small reader was ported instead. Milestone 1
therefore has no dependencies.

### 6. Parity tests against the prototype

`crates/schema-bridge-core/tests/fixtures/*.expected.txt` were produced by
running the prototype's `buildModel` on the demo exports and printing the model
line by line. The Rust tests print their model in the same format and must
match exactly. To regenerate an expected file after an intended behaviour
change, run the prototype on the same demo file and print it in the format of
`summary` in `tests/import_parity.rs`.

### 7. Sample data is generic and made up

Schema Bridge is a general-purpose tool: it works on whatever two schemas the
user loads, and nothing about a particular comparison is built into it. The
sample and test data in `demo/` describe two invented databases (Workshop and
Catalogue) and are read by exactly the same code as a user's own exports.

Real schemas used to verify the application during development are kept
outside the repository. No real organisation's schema, results, data or
credentials are part of the code, the tests, the sample data or the UI.

### 8. Small, known differences from the prototype

- **Table order.** The prototype sorts table names with the browser's locale
  rules; Rust sorts case-insensitively. Both give the same order for ordinary
  names; unusual names (for example `a_z` and `ab`) may appear in a different
  order. Only display order is affected.
- **Malformed constraint rows** without a referenced table get an empty table
  name instead of the prototype's `undefined`.

### 9. Type names are normalised only where a name means something else

At import, types are lower-cased, and three names are translated because they
mean something different in their system: Oracle `DATE` (includes a time),
SQL Server `timestamp` / `rowversion` (binary row version), MySQL `float`
(4 bytes). Full type recognition and compatibility are in `types.rs` (decision 11).

### 10. Ownership and licence

The repository owner decides the licence, authorship and releases. Schema
Bridge is released under the MIT License (see [LICENSE](../LICENSE)),
copyright (c) 2026 Iqra Naz. *Updated 24 October 2026: originally no licence
had been chosen.*

### 11. Type recognition is tested against the prototype

`types.rs` recognises type names from all five systems and judges whether a
value of one type fits a column of another (same, safe, convert, loss,
incompatible). `tests/fixtures/types.input.txt` and `compat.input.txt` list
94 types and 65 type pairs; the matching `*.expected.txt` files hold the
prototype's answers, and the Rust results must match them exactly.

### 12. Two deliberate differences from the prototype's type rules

- **Lists are checked first.** The prototype judged a list type against a
  single-value type of the same kind (for example `integer[]` into `integer`)
  as "safe", because its "same kind of value" shortcut ran before the list
  check. Rust checks lists first, so this is reported as incompatible.
- **Neutral wording.** The note for times without a time zone no longer
  suggests a specific time zone; it asks which zone the values were recorded in.

### 13. The application is a Dioxus web app; all logic stays in the library

The application (`crates/schema-bridge-app`) is a Dioxus 0.7 web app, run with
`dx serve` and opened at `http://localhost:8080`. It runs in the browser as
WebAssembly, so files loaded into it stay on the user's computer and no server
is involved. The same code can later be built as a desktop app
(`dx serve --desktop`, `desktop` feature).

The app only displays what the library computes. Its state logic (the two
slots, loading files, the sample schemas) lives in
`schema-bridge-core/src/workspace.rs`, so it is covered by `cargo test`.
Because the app is built for the browser target, the workspace's default
members are the library only: `cargo test` and `cargo check` at the root cover
the library, and `cargo check -p schema-bridge-app --target
wasm32-unknown-unknown` checks the app.

### 14. Application design rules

- The prototype's workflow, terms and information are kept; the visual design
  is new: a workflow navigation on the left (stages not built yet are shown
  but disabled), cards for the source and target, tables for figures.
- Colour roughly follows 60-30-10: neutral page, panels and navigation, one
  accent for primary actions and the active step. Status colours (success,
  warning, error, info) are used only for status. Source and target each keep
  one identifying colour.
- System fonts only, so the app makes no external requests.
- Light and dark themes follow the operating system; keyboard focus is always
  visible; motion is limited to short colour transitions and respects
  "reduce motion".
- The app starts empty. Sample schemas are loaded only on request.
- *Updated 24 October 2026 (visual design pass, no change in behaviour):*
  all colours come from tokens in `assets/main.css`: neutrals, one accent,
  and semantic triples (colour, soft background, border) for success,
  warning, high, error and info, so the five severities are distinct
  (Blocking is the only filled pill). The target's identity colour is
  violet, so it is no longer the same blue as primary actions; the source
  stays teal. The navigation is a stepper showing each stage as current,
  complete, ready or waiting for data, with counts coloured by the most
  serious finding. The top bar shows the comparison (source → target) and
  a System / Light / Dark theme switch. Empty states, a "Reading file…"
  state and a short fade between stages were added; below 900 px the
  stepper becomes a horizontal strip.
- *Updated 24 October 2026 (second design pass, no change in behaviour):*
  depth replaces most borders (page, navigation surface, elevated cards,
  inner panels); the brand mark joins the source teal and target violet; the
  Load stage opens with what Schema Bridge does and the sample schemas as the
  primary action; the databases are panels with an identity header, engine,
  state and statistics, joined by a "bridge" that carries the matching
  result; compact summary cards (headline, segmented bar, counts) are shared
  by Load and Report; the navigation shows progress (completed, current,
  available, waiting for data) with a progress bar; uppercase labels were
  removed; on narrow screens the order is source, bridge,
  target, analysis, next step.

### 15. Problem detection is a port of the prototype's checks, tested for parity

`checks.rs` ports the prototype's `dbChecks`: the same rules, the same
severities and the same wording, in the same order. Expected findings in
`crates/schema-bridge-core/tests/fixtures/*.checks.expected.txt` were produced
by running the prototype on the same export files and printing one line per
finding (`severity | area | object | title | explanation | fix`); the Rust
findings must match them word for word.

The two samples trigger only part of the rules, so a small made-up `library`
database (`tests/fixtures/checks-coverage/library.sql`) was created on
PostgreSQL 16 and exported with `export-queries/postgresql.sql`. Together
with the samples it triggers every rule. To regenerate an expected file after
an intended change, run the prototype's `dbChecks(buildModel(file))` on the
same export and print it in the format of `lines` in `tests/checks_parity.rs`.

Additions and small differences:

- **Rule ids.** Every finding carries a rule id from the specification's rule
  catalogue (§7), which the prototype does not have. Two prototype checks are
  missing from the catalogue and got their own ids: `NAME-LINK-COLUMN` (a link
  column whose name suggests a related table) and `KEY-MIXED-GENERATORS`
  (different ID generator functions).
- **Order.** As in the prototype, the combined list is sorted by severity,
  then source before target, otherwise in the order the checks report them.
- **Unnamed constraints and indexes** appear with an empty name where the
  prototype would print `undefined`.
- **Deferred parts of the prototype's Problems tab:** the "Fitting them
  together" filter (integration issues, milestone 5) and "Copy as table"
  (export, milestone 6).

### 16. The Problems screen

The Problems stage shows severity tiles, filters by database and severity
(each with its count), and one card per finding: severity, database, area,
rule id, title, object, explanation and "What to do". The Load stage gets a
short "Problems found" summary with a link to the stage, and the navigation
shows the number of findings. The filter logic (`FindingFilter`) lives in the
library and is tested there. Findings are recomputed from the loaded schemas;
nothing is stored separately.

### 17. Matching is a port of the prototype's, tested for parity

`matching.rs` ports `parseDict`, `toks`, `dice`, `colSim`, `tableScore`,
`matchTables` and `mapColumns`: the same scores, thresholds (0.55 for tables,
0.65 for columns), order of strength (your choices, then word-list pairs and
skip rules, then the score) and notes. `tests/fixtures/plan/*.expected.txt`
were produced by running the prototype engine on the sample pair in three
situations: with the sample word list, with manual choices
(`sample-overrides.json`), and with no word list. The same files also hold
the prototype's integration findings, load order and SQL, which milestone 5
is tested against. To regenerate them, run the prototype's `analyse()` on the
same inputs and print the result in the format of `summary` in
`tests/matching_parity.rs`.

Additions and small differences:

- **A sample word list** (`demo/sample-word-list.txt`) is loaded with the
  sample schemas, as the prototype loads one with its example. It is removed
  when the user's own files come in, unless the user edited it.
- **Lines that are not a rule** are listed under the word list editor
  instead of being ignored silently (specification §9, "with validation").
- **Skip patterns** treat every character except `*` literally (the
  prototype's pattern also treated `?` as a regular-expression character).
- **Choices made by hand** are kept until a file is loaded or cleared, or
  the databases are swapped, as in the prototype. Saving them across
  sessions comes with the workspace file (FR-04, FR-12).

### 18. Workflow stages

The navigation follows the workflow Load schemas → Problems → Matching →
Integration issues → Plan → Draft SQL → Export. Plan and draft SQL are
separate stages (one tab in the prototype) so the SQL, which is the longest
output, has its own screen. The word list is a view of the Matching stage,
because it only changes matching.

### 19. Integration issues, load order and draft SQL are ports, tested for parity

`integration.rs`, `plan.rs` and `sql.rs` port the prototype's
`integrationChecks`, `loadOrder`, `castExpr`, `sqlFor` and the script of its
plan tab. `tests/plan_parity.rs` compares the findings, the load order and
the whole draft script with the prototype's on the three situations of
decision 17.

Additions and small differences:

- **Rule ids** for integration findings (`MAP-REQUIRED-LINK`,
  `MAP-REQUIRED-VALUE`, `MAP-RELATIONSHIP`, `MAP-DUPLICATES`,
  `MAP-TYPE-FIT`, `MAP-EMPTY-VALUES`, `MAP-NO-SOURCE-ID`,
  `MAP-MANY-TO-ONE`, `MAP-NO-TARGET`), one per check the specification lists
  in §7 under "Integration checks".
- **No built-in time zone.** The prototype converted times without a time
  zone with `AT TIME ZONE 'Europe/Berlin'`. The draft now writes
  `AT TIME ZONE '<source time zone>'` with a TODO, so nothing about one
  organisation is built in and the statement fails until someone decides.
  The fixtures were generated with this one change applied.
- **Cycles.** As in the prototype, tables on a cycle of links come last,
  marked; the Plan stage names them and suggests loading in two passes
  (specification §8, step 4). The links that form the cycle are not listed
  yet.
- **The draft is never run.** Schema Bridge has no database connection. The
  script reads the source through a staging schema, opens a transaction and
  ends with `COMMIT` commented out; its connection options are placeholders
  (`'<password>'`), never real values. Copy and Save as .sql stay on the
  user's computer.
- **Not yet (specification §8):** the CSV-staging alternative is mentioned
  in the script but not generated; SQL for targets other than PostgreSQL
  (FR-17) is a note that asks the reader to adapt the statements.

### 20. Exports and the workspace file

- **Exports** (`export.rs`, FR-18): findings as CSV and as tab-separated
  text to paste into a spreadsheet or ticket (the prototype's "Copy as
  table"), the mapping matrix as CSV, and a hand-over report in Markdown.
  CSV fields that a spreadsheet would read as a formula (starting with `=`,
  `+`, `-` or `@`) get a leading `'`, so opening the file never runs
  anything. Excel and HTML output (also FR-18) are not done; CSV opens in
  Excel, and Markdown converts to HTML in most tools.
- **Workspace file** (`workspace_file.rs`, FR-04, FR-12): JSON holding both
  structure exports as loaded, the names, the word list and every choice
  made by hand, with a format name and version so other files are rejected
  with a reason. It holds structure only, never table contents or
  passwords. Choices therefore last across sessions when the workspace is
  saved and reopened; nothing is kept in the browser's storage.
- **serde and serde_json** are the core library's first dependencies
  (decision 5 kept milestone 1 dependency-free). Writing and reading JSON
  by hand would be more code to get wrong; both crates were already part of
  the application's build through Dioxus.
- **Word list files** (FR-11): the word list editor loads a text file into
  the editor (applied with the usual button) and saves the editor's text.
- **How to make an export** is a panel on the Load stage with the queries
  from `export-queries/` and the steps for each system's usual tool (the
  prototype's "How to export" tab). A test checks that every query only
  reads.

### 21. One registry for everything system-specific

Database-specific knowledge was spread over the importer (type names),
the checks (the validation statement, how to add descriptions, SQLite
having none), the draft SQL (foreign data wrappers), the display names and
the export queries. It is now in one registry, `systems.rs`, with one
`DatabaseSystem` entry per system; the rest of the library asks it by the
system code and falls back to neutral behaviour for systems it does not
know (no foreign data wrapper means CSV staging; an unknown code is shown
as it is). Adding a relational system that can produce the one-file export
is one entry plus its query ([adding-a-database.md](adding-a-database.md)).
Behaviour for the five known systems is unchanged; all parity tests pass.

Deliberately not done yet: an interface for importers of other input
formats (development plan, milestone 7) and model support for time-series
concepts (milestone 8). MySQL and MariaDB share one entry until their
differences need two.

### 22. Release pass: review tools, report, filters, performance

- **Review states and evidence.** Automatic column matches keep their score
  parts (`Evidence`); tables below 70 % and columns below 85 % are
  *uncertain*. "Accept" stores the tool's choice as the user's, so accepted
  matches count as "changed by you". A column the user removed is marked
  `manual` without changing `how`, so the prototype parity output stays the
  same. Thresholds are in `matching.rs`.
- **Filters in the core.** `FindingFilter` (text, database, severity, rule),
  `TableFilter`, `ColumnFilter` and `TableSort` live in the library and are
  tested there; screens only hold them.
- **Report stage.** A *Report* stage before *Export* gives the summary view;
  the full report is one document model rendered to Markdown and to a
  self-contained HTML page, so both formats say the same. The plan's six
  steps moved into the core (`report::plan_steps`) and are shared by the
  Plan screen and the report.
- **Saved comparison.** The existing workspace file is the saved session;
  nothing is stored in the browser automatically, so closing the page
  never leaves data behind.
- **Performance.** Table scoring prepares each name's words once
  (identical results, about 27× faster); the application computes the
  analysis once per change (memos in `state.rs`); long lists render in pages.
- **Time-series extension points** (`TableKind`, `ColumnRole`,
  `RetentionPolicy`) were added to the model, defaulting to relational and
  not yet used by any importer or rule.
- **Documentation tests.** `tests/user_guide.rs` checks the numbers quoted
  in the user guide; the type examples in `type-compatibility.md` are tested
  in `types.rs`.

### 23. One import boundary: schema inputs and importers

Milestone 7. A workspace slot used to hold the CSV export files
(`ExportFiles`) directly, so every other input format would have needed
changes in the slot, the saved comparison and the application. Now:

- A slot holds a `SchemaInput` (`input.rs`): `Empty`, or one variant per
  input format. Today the only format is `CsvExport(ExportFiles)`.
- Every format has an `Importer` with a stable id, which recognises its
  files, adds them to an input, builds the neutral `Schema`, and saves and
  opens its input. Adding a file asks the importers in order which one
  recognises it.
- `CsvExportImporter` (id `csv-export`) delegates to the unchanged
  `import.rs`, so CSV import behaves exactly as before; a test compares the
  old and new paths on every sample export, and all parity tests still use
  `ExportFiles` and `build_schema` directly.
- The saved comparison is **format version 2**: each slot records
  `importer` and `input`. Version 1 files (0.1.0) still open, because their
  slots held the CSV parts under the same names the CSV importer reads. An
  unknown importer id is refused with a message that names it and suggests
  updating or reloading the export.
- The load notice now carries a short description from the importer
  ("one-file export") instead of the CSV `ExportKind`, so it fits any format.

No behaviour changed: the same files are accepted and refused with the same
messages, adding, replacing and clearing work as before, and the analysis
results are identical.

### 24. Time-series schemas in the neutral model (InfluxDB first)

Milestone 8. InfluxDB is schema-on-write: buckets with a retention period
hold measurements, a measurement's points have a nanosecond `time`, tags
(indexed text that identifies a series) and typed fields. There are no keys,
links, constraints or explicit indexes, and a point with the same series and
time silently replaces the earlier one.

- **One neutral model.** A measurement is a `Table` with
  `kind: TableKind::Measurement`; its columns carry `ColumnRole::Timestamp`,
  `Tag` or `Field`; buckets are `RetentionPolicy` entries, and a new
  `Table::series` holds the series cardinality (never mixed with `rows`).
  Series identity (tags + time) is derived from the roles when a rule needs
  it, not stored as a primary key, so no relational rule mistakes it for one.
- **Interchange format, not a native format.** Schema Bridge defines a
  normalised time-series schema export (CSV: `record_type, bucket,
  retention, measurement, key, data_type, series, dbms`), produced with the
  provided Flux script (InfluxDB 2.x, 1.8 with Flux) or SQL query
  (InfluxDB 3), or by any other tool. Flux annotated CSV is accepted
  unchanged. Flux is a way to produce the export: it is neither an importer
  nor a database system.
- **Importer.** `InfluxSchemaImporter` (id `influx-schema-csv`) behind the
  milestone-7 `Importer` interface. It is asked before the CSV structure
  export importer because the latter accepts any header with
  `record_type`; recognition requires `measurement` and `key` and no
  `table_name`.
- **Isolated rules.** Relational rules run only on tables of kind `Table`;
  time-series rules (`TS-*`) only on measurements; integration rules for
  time series (`MAP-TS-*`) only on mappings that involve a measurement. A
  relational-only comparison gives exactly the same results as before.
- **Conservative conclusions.** Tags + time are offered as the identity to
  review (a finding when the target has no unique rule on them), never
  turned into a primary key. Nanosecond times into relational time types are
  shown as potential information loss. Unknown field types stay unknown and
  are reported. PostgreSQL has no retention period, so a finite source
  retention is reported as a decision, not converted.
- **SQL.** InfluxDB as a source: the usual draft with CSV staging (no
  foreign data wrapper is assumed). InfluxDB as a target: no SQL is
  generated; Schema Bridge does not write line protocol.
- **Fixed on the way:** for a source system without a known foreign data
  wrapper the draft used to keep an `IMPORT FOREIGN SCHEMA … FROM SERVER`
  statement for a server it had not created; it now writes CSV staging
  tables and `\copy` commands. The five relational systems all have a
  wrapper, so their drafts are unchanged.
- **No source-ID finding for measurements.** `MAP-NO-SOURCE-ID` assumes a
  source row has an ID to keep; a point has none (its identity is tags +
  time, covered by `MAP-TS-SERIES-KEY`), so the rule skips measurements.

