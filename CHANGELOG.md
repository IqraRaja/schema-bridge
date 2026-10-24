# Changelog

All notable changes to Schema Bridge. Dates are when the change was made;
releases are tagged by the repository owner.

## [Unreleased]

### Architecture

- One import boundary: a database slot holds a general schema input, and
  every input format has an importer that turns it into the neutral model.
  The CSV structure exports are the first importer and behave exactly as
  before (milestone 7, decision 23).
- Saved comparisons use format version 2, which records the importer of each
  database. Files saved with 0.1.0 (version 1) still open; a file naming an
  importer this version does not have is refused with a clear message.

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
  (`systems.rs`) and extension points for time-series concepts in the model.

### Known limitations

See the README's "Limitations" section.
