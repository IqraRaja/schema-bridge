# Architecture

How Schema Bridge is put together, for developers. User-facing explanations
are in the [README](../README.md) and the [user guide](user-guide.md);
decisions and their reasons are in [decisions.md](decisions.md).

## Crates

| Crate | Contains | Built for |
| --- | --- | --- |
| `schema-bridge-core` | All logic: import, model, types, checks, matching, integration, plan, SQL, report, export, workspace. No UI, no network, no database access. Depends on `serde`/`serde_json` only. | native (tests) and WebAssembly |
| `schema-bridge-app` | The Dioxus 0.7 application: screens, styles, browser actions (copy, download). | WebAssembly (`dx serve`) |

The workspace's default member is the core, so `cargo test` and
`cargo check` at the root cover it; the app is checked with
`--target wasm32-unknown-unknown`.

## Data flow

```
 structure export (CSV)            ← made by the user with a read-only query
        │
        ▼
 import.rs  ──uses──▶ systems.rs    database-specific type names
        │
        ▼
 model.rs                           neutral Schema → Table → Column, keys, links,
        │                           indexes; time-series extension points
        ▼
 checks.rs     problems inside one database         (uses types.rs, systems.rs)
 matching.rs   word list, table/column matching, evidence, review states, filters
 integration.rs  problems in fitting the two together
 plan.rs       load order
 sql.rs        draft SQL (text only)                (uses systems.rs)
 report.rs     summary, plan steps, report (Markdown/HTML)
 export.rs     findings and mapping CSV
        │
        ▼
 workspace.rs        the two slots, word list, choices, Analysis, FindingFilter
 workspace_file.rs   save / open the comparison (JSON)
        │
        ▼
 schema-bridge-app   displays it; sends the user's actions back
```

## The core library, module by module

| Module | Responsibility |
| --- | --- |
| `csv_reader` | Small CSV reader that keeps "quoted or not" (an unquoted `NULL` is no value) |
| `import` | Recognises export files, builds the `Schema` |
| `model` | `Schema`, `Table`, `Column`, keys, links, indexes, statistics; `TableKind`, `ColumnRole`, `RetentionPolicy` for time series |
| `types` | Type recognition (`type_info`) and compatibility (`compatibility`, `Fit` with labels and meanings) |
| `systems` | Registry of database systems: names, type overrides, validation statements, description fixes, foreign data wrappers, export queries |
| `checks` | The 20 rules inside one database (`Finding`, `Severity`) |
| `matching` | Word list, tokens, scores, `match_tables`, `map_columns`, `Evidence`, `Review` states, `TableFilter`, `ColumnFilter`, `TableSort` |
| `integration` | The 9 `MAP-*` rules |
| `plan` | Load order (`LoadStep`), cycles |
| `sql` | Draft SQL per table and the whole script |
| `report` | `Summary`, `PlanSteps`, report document (`Block`) rendered to Markdown and HTML |
| `export` | Findings CSV/table, mapping CSV |
| `workspace` | `Workspace` (slots, word list, overrides), `Analysis`, `FindingFilter`, samples |
| `workspace_file` | JSON save/open with format name and version |

## The application

| File | Screen or role |
| --- | --- |
| `main.rs` | Shell: top bar, workflow stepper, stage switching, theme |
| `state.rs` | Shared derived values: `Analysis` and findings as memos, computed once per workspace change and provided as context |
| `views.rs` | Load stage: first-run explanation, the two database panels joined by the bridge, analysis summary, next step, side-by-side figures, how to make an export |
| `widgets.rs` | Summary cards (headline, segmented bar, counts) shared by the Load and Report stages |
| `problems.rs` | Problems stage; shared finding cards and filters |
| `matching.rs` | Matching stage: table list, detail, column grid, word list |
| `integration.rs`, `plan.rs`, `report.rs`, `export.rs` | The other stages |
| `browser.rs` | Clipboard and downloads (local only) |
| `icons.rs` | Inline line icons (no external requests) |
| `assets/main.css` | The design system: tokens, components, themes, responsive rules |

Screens never compute the analysis themselves; they read the memos from
`state.rs`. Lists of tables (200) and findings (100) render in pages.

## What is database-neutral

The analysis never branches on the database system. Where wording or SQL
must depend on the system, the code asks the `systems` registry by the code
in `Schema::dbms`, with neutral fallbacks for unknown systems (CSV staging
instead of a foreign data wrapper, generic instructions):

| Need | Registry field | Used by |
| --- | --- | --- |
| Display name | `name` | everywhere |
| Type names that mean something else in this system | `type_overrides` | `import` |
| Statement that validates an unchecked link | `validate_link` | `checks` (`LINK-NOT-VALIDATED`) |
| How to add column descriptions, or none | `description_fix` | `checks` (`DOC-COVERAGE`) |
| Foreign data wrapper for reading it as a source | `foreign_data_wrapper` | `sql` |
| Read-only export query and steps | `export` | Load stage |

Deliberately specific: `sql` writes PostgreSQL syntax (FR-16) and says so for
other targets; the four-file export is a PostgreSQL format.

## Extension points

- **A relational system with the one-file export**: one `systems` entry and
  an export query ([adding-a-database.md](adding-a-database.md)).
- **New type names**: the vocabulary in `types.rs`, with parity fixture lines.
- **A new input format** (DDL script, JSON snapshot, time-series schema): an
  importer that produces `model::Schema`. *Today* a workspace slot stores
  `ExportFiles` (the CSV formats), so a second format also needs the slot and
  the workspace file to hold a general input (development plan, milestone 7).
- **Time-series databases**: `TableKind::Measurement`, `ColumnRole`
  (`Timestamp`, `Tag`, `Field`) and `RetentionPolicy` exist in the model and
  default to relational. No importer fills them and no rule reads them yet
  ([databases.md](databases.md#time-series-databases), milestone 8).

## Performance

Matching splits every table and column name into words once and compares
prepared word sets without allocating (`PreparedTable`, `dice_sorted`). Two
schemas of 400 tables × 12 columns are analysed in about 0.3 s (release) or
1.2 s (debug); `tests/large_schema.rs` guards this.

## Testing approach

Unit tests next to the code and integration tests in
`crates/schema-bridge-core/tests/` (see [CONTRIBUTING.md](../CONTRIBUTING.md#tests)).
The engine is compared with the original browser prototype on the sample
pair (parity tests, decisions 6, 15, 17 and 19). No test needs a database,
network access or credentials.
