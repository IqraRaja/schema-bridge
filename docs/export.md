# Export and saved comparisons

Everything is produced in the browser and offered as a download (or copied
to the clipboard); nothing is uploaded. Files are named after the two
databases, for example `workshop-sample-into-catalogue-sample-report.html`.
The code is `report.rs`, `export.rs` and `workspace_file.rs` in
`crates/schema-bridge-core/src/`.

## Comparison report

On *Report* and *Export*: **HTML** (one self-contained page, no external
files, prints to PDF from the browser) or **Markdown** (for wikis and
tickets). Both contain the same sections:

1. **Summary**: tables (matched, uncertain, changed by hand, no place, staying),
   columns (mapped by type result, not mapped, never copied, uncertain,
   changed by hand), problems by severity per database and for fitting them
   together.
2. **Databases**: system, tables, columns, links, primary keys, estimated rows.
3. **Unresolved items**: fix first, decide, review, draft SQL.
4. **Integration risks**: blocking and high integration issues with what to do.
5. **Problems found**: every finding with severity, database, rule, object,
   explanation and fix.
6. **Mapping**: every table decision with its reason, and for each matched
   table every column with target, type result and explanation, reason and
   confidence, and notes.
7. **Changes made by hand**: every table and column choice you made.
8. **Type conversions**: every mapped column whose type differs, with its
   result and plain explanation.
9. **Integration plan**: the six steps, including the load order.
10. **Draft SQL**: the whole script, with the safety note.
11. **Assumptions and limitations**: what was and was not compared.

With only one database loaded the report contains the problems and says
what is missing. To see an example without the app:
`cargo run -p schema-bridge-core --example sample_report > report.html`.

## Findings

- **Save as CSV**: one finding per row: Severity, Database, Area, Rule,
  Object, Problem, What it means, What to do. All findings: inside each
  database and in fitting them together.
- **Copy as table**: the same columns, tab-separated, to paste into a
  spreadsheet or ticket.

## Mapping (CSV)

One row per source column: Source table, Source column, Source type, Target
table, Target column, Target type, Result, Type fit, Type explanation, Matched
by (automatic, word list, you), Confidence, Changed by you, Notes. Plus one
row per required target column that nothing fills ("needs a decision"), and
rows for the columns of tables without a target.

## Draft SQL

**Save as .sql** on *Draft SQL* or *Export*. See
[integration-plan.md](integration-plan.md#draft-sql) for its contents,
safety and limits.

## CSV safety

CSV files are written with standard quoting (commas, quotes and line breaks
inside fields). A field that a spreadsheet would treat as a formula (starting
with `=`, `+`, `-` or `@`) gets a leading `'`, so opening the file never runs
anything.

## Saved comparison

**Save comparison** (top bar or *Export*) writes `….schema-bridge.json`:

```json
{
  "format": "schema-bridge-workspace",
  "version": 1,
  "source": { "name": "Workshop", "combined": "record_type,table_schema,…" },
  "target": { "name": "Catalogue", "combined": "…" },
  "using_samples": false,
  "word_list": "machine = asset\n…",
  "table_choices": [
    { "source_table": "main.booking", "choice": "target", "target_table": "public.session" }
  ],
  "column_choices": [
    { "source_table": "main.machine", "source_column": "room", "target_column": "location_id" }
  ]
}
```

- It holds both exports as loaded (one-file, or the four parts), the names,
  the word list and every choice (`choice` is `target`, `none` or `stays`; a
  missing `target_column` means "not copied").
- **Open workspace…** restores everything; the analysis is recomputed and is
  the same as before (a test checks this).
- Files that are not workspaces are refused with a reason; a file from a
  newer format version asks you to update Schema Bridge.
- It contains structure only: no table contents, no passwords, no
  connection details. It is plain text you can inspect.
- Nothing is saved in the browser automatically: reloading the page starts
  empty. Keep the file if you want to continue later.
