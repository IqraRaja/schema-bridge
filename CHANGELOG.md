# Changelog

All notable changes to Schema Bridge. Dates are when the change was made;
releases are tagged by the repository owner.

## [Unreleased]

### Interface (UI-1)

- **Readiness** in the navigation: the report's count of items to resolve
  before a test load, and a check per stage that is clear only when nothing
  is open there (previously a stage counted as complete once opened). Each
  stage says what is still open.
- **Previous / Next** at the bottom of every stage; changing stage starts at
  the top of the new stage and moves the keyboard focus to its heading.
- **Confirmations** before loading a sample or opening a saved comparison
  over your own work, and before Clear, Swap and Reset all my choices when
  choices made by hand would be lost.
- **Notices** after every saved file and copy.
- The colour theme is remembered in the browser (the only thing stored
  there).

### Interface (UI-2)

- **Drag and drop**: export files can be dropped onto the Source and Target
  panels (one or several at a time); dropping onto a loaded panel replaces
  its export. A file dropped beside the panels no longer replaces the
  application in the browser.
- **Start screen**: a clear "Use your own databases" path (Add your exports,
  How to make an export file), two equal sample cards (relational and time
  series) and Open workspace… to continue earlier work.
- **Error card** for a file that is not an export: the file, the reason, the
  accepted exports, Show me how to export, and a dismiss button.
- **Reading state**: the panel names the file being read and its actions
  wait until it is done.
- Clearer badges (the Sample data badge no longer looks like a link), and a
  short explanation on hover for each panel figure.

### Fixed

- The message for a file that is not an export now names the help section
  correctly ("How to make an export file").

## [0.1.0] - 2026-10-24

First release.

### Workflow

- **Load schemas**: one-file structure exports for PostgreSQL, MySQL/MariaDB,
  SQL Server, Oracle and SQLite, and the older four-file PostgreSQL export;
  read-only export queries with steps per system; sample schemas on request;
  swap source and target.
- **Problems**: 20 rules inside each database with severity, explanation and
  fix; search, and filters by database, severity and rule type.
- **Matching**: table and column matching from names, a word list,
  descriptions and types; reason and confidence for every automatic match;
  search across table and column names; filters for matched, no place,
  uncertain, changed by you and staying tables; sorting; accept, change,
  remove and reset any table or column match; word list editor with line
  validation, load and save.
- **Type conversions**: every mapping classified as Same, Safe, Requires
  conversion, Potential information loss or Incompatible, with a plain
  explanation.
- **Integration issues**: 9 rules on how the two databases fit together,
  with the same search and filters.
- **Plan**: six steps, load order with required parent tables and cycles.
- **Draft SQL**: `INSERT … SELECT` per table in load order with casts,
  lookups and `TODO`s; never run by the application.
- **Report**: the whole comparison on one page, and as a self-contained HTML
  or Markdown report with summary, problems, mappings, manual changes, type
  conversions, plan, draft SQL, risks, assumptions and unresolved items.
- **Export**: findings (CSV, table to paste), mapping (CSV), draft SQL, and a
  saved comparison file that reopens with every choice.

### Application

- Design-token based interface with light and dark themes: a first screen
  that explains the workflow, database panels joined by a bridge that shows
  the matching result, summary cards, a navigation that shows progress
  (completed, current, waiting for data), clear source/target colours, and a
  layout reorganised for narrow screens.
- Analysis computed once per change; large schemas (400 × 400 tables)
  analysed in about a second in a development build; long lists shown in
  pages.

### Architecture

- Database-neutral core with one registry for system-specific knowledge
  (`systems.rs`); time-series concepts (measurements, tags, fields,
  retention) live in the same neutral model.
- One import boundary: a database slot holds a general schema input, and
  every input format has an importer that turns it into the neutral model.
  The CSV structure exports are the first importer and behave exactly as
  before (milestone 7, decision 23).
- Saved comparisons use format version 2, which records the importer of each
  database. Files in format version 1 still open; a file naming an
  importer this version does not have is refused with a clear message.

### Time series (milestone 8, decision 24)

- **InfluxDB schemas** through Schema Bridge's time-series schema export
  (a normalised CSV of buckets, retention, measurements, tags, fields, field
  types and series counts), with a Flux script (InfluxDB 2.x, 1.8) and an SQL
  query (InfluxDB 3), not yet run on real servers. Flux annotated CSV is
  accepted; mistakes are refused with their line number. New importer
  `influx-schema-csv`; measurements, tags, fields and retention live in the
  one neutral model.
- **Problems:** six `TS-*` rules (no fields, type conflicts, unknown types,
  tag/field key clashes, high cardinality, unsigned fields). Relational
  rules no longer run on measurements.
- **Matching:** nanosecond times into relational time types are shown as
  potential information loss; unknown field types and type conflicts are
  never guessed; tag ↔ field role changes are noted.
- **Integration issues:** `MAP-TS-SERIES-KEY` (point identity without a
  unique rule, never added automatically), `MAP-TS-RETENTION` (limited
  history, shorter target retention), `MAP-TS-ROLE-CHANGE`.
  `MAP-NO-SOURCE-ID` is not reported for measurements.
- **Draft SQL:** CSV staging for an InfluxDB source; for a time-series target
  no SQL is generated, with an explanation in the plan, Draft SQL stage,
  Export and report.
- **Screens:** *Time series* badge; measurements, tags, fields and series on
  the database panel; retention; time/tag/field markers in the column grid;
  the *Time series* area in problems; the SQL notice. The report adds a time-
  series table and assumptions.
- **Sample:** *Try the time-series sample (InfluxDB → PostgreSQL)*, Plant →
  Readings, with its word list.

### Fixed

- For a source system without a known foreign data wrapper, the draft SQL
  kept an `IMPORT FOREIGN SCHEMA … FROM SERVER` statement for a server it
  never created. It now creates CSV staging tables with `\copy` commands.
  The drafts for the five relational systems are unchanged.

### Known limitations

See the README's "Limitations" section.
