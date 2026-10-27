# User guide

Schema Bridge analyses the structure of two databases and helps you plan an
integration or migration: it reports findings and incompatibilities,
suggests mappings for you to review, lists the decisions that are still
open, and drafts a plan and SQL for a test load. It never moves data, and the
design decisions stay with you.

This guide walks through a complete comparison with the built-in sample
schemas, then explains each stage in detail. Start the application first
([installation](installation.md)) and open http://localhost:8080.

## Ten-minute walk-through with the sample schemas

The samples describe two made-up databases: **Workshop**, a SQLite database
of machines, operators and runs (the *source*), and **Catalogue**, a
PostgreSQL catalogue of assets, people and sessions (the *target*). They
contain deliberate flaws so every feature has something to show; the
[demo README](../demo/README.md) lists them.

1. **Load.** On the first screen choose the **Relational sample** card. Both
   database panels fill in: SQLite with 5 tables on the left (teal), PostgreSQL
   with 7 tables on the right (violet). The bridge between them now reads
   *4 of 5 tables matched · 11 integration issues*, three summary cards show
   tables, columns and findings at a glance, and a blue band suggests the next
   step. The top bar shows *Workshop (sample) → Catalogue (sample)*, and step 1
   in the navigation gets a check mark.
2. **Schema checks.** Open *Schema checks* (16 findings). The red tile says there is
   1 Blocking finding: `asset.parent_asset_id` links to `location` instead of
   `asset`. Type `serial` in the search box: one finding remains
   (`machine.serial_no` identifies a machine but has no unique rule). Clear
   the search, choose *LINK-IMPLICIT* in the rule list: the link
   `machine_function.machine_id` that nothing checks.
3. **Matching.** Open *Matching*. Four of five tables have a place; `booking`
   stays in Workshop because the sample word list says so. `machine` is
   selected: it goes into `asset`, 76 % similar.
   - The red box says `asset` needs an `asset_type_id` that Workshop does
     not have: someone must decide it.
   - The row `id → uuid` is marked *Automatic · 75 %* and *Incompatible*:
     the names agree but an integer cannot become a UUID. Press **✕** to
     remove that mapping; the row now says "You chose not to copy this
     column". Press **↺** to undo.
   - Choose the chip **Uncertain** in the table list: these are the matches
     worth a second look. Select `operator`, which matched only 60 %, and
     press **Accept match**: it now counts as your choice.
4. **Integration issues.** 11 issues, the most serious first: the missing
   `asset_type_id` (Blocking), duplicates `code` would reject, and the
   person's first and last name that Workshop does not have.
5. **Plan.** Six steps. Step 4 shows the load order: `asset_type` must already
   hold rows (dashed), then `person ← operator`, `session ← run`,
   `asset ← machine`, `capability ← machine_function`.
6. **Draft SQL.** The script reads Workshop through a staging schema and
   loads the tables in that order. Highlighted lines are `TODO`s: decisions
   from the plan.
7. **Report.** The whole comparison on one page: *4 items to resolve before a
   test load*, tables, columns and findings in numbers, risks and the
   unresolved list. **Save report (HTML)** produces a page you can send or
   print.
8. **Export.** Save the findings or the mapping as CSV, or **Save comparison**
   to continue later. Reopen it with **Open workspace…** on the Load stage.

### The time-series sample (InfluxDB → PostgreSQL)

The **Time-series sample** card on the first screen (or **Time-series
sample** on the Load stage) loads **Plant**, a made-up
InfluxDB bucket, and **Readings**, a PostgreSQL database for its sensor data.

1. **Load.** The Plant panel shows a *Time series* badge, 3 measurements,
   6 tags, 9 fields and 622 series, and *Retention plant: 30 days*. The
   bridge reads *2 of 3 measurements matched*.
2. **Schema checks.** Three findings in Plant, area *Time series*: `pump.speed_rpm`
   has two types, `boiler.firmware_version` has an unknown type, and
   `energy_meter.total_kwh` is unsigned.
3. **Matching.** `boiler → readings` comes from the word list, `pump →
   pump_readings` is automatic, `energy_meter` has no place. Columns are
   marked **time**, **tag** or **field**. `time → recorded_at` is potential
   information loss: nanoseconds become microseconds.
4. **Integration issues.**
   - `readings` has no unique rule on (site, unit_id, recorded_at):
     `MAP-TS-SERIES-KEY`, to review, not added automatically.
   - Both mappings get `MAP-TS-RETENTION`, because Plant keeps only 30 days.
5. **Plan.** Step 3 lists the open decisions and, below them, the **design
   decisions that stay with you**: the target tables, tags and fields, how
   time is stored, indexes, storage and retention, performance and
   compression, and whether a time-series extension such as TimescaleDB fits.
   Schema Bridge shows the differences; it does not choose the model.
6. **Draft SQL.** CSV staging tables for `boiler` and `pump` with `\copy`
   commands, then the inserts: a draft for one possible model (the sample's
   hand-written target), not a recommendation. Press **⇄ Swap** to make
   InfluxDB the target: the Draft SQL stage then explains that no SQL or line
   protocol is generated.

Details: [time series](time-series.md#design-decisions-that-stay-with-you).

## The stages in detail

### 1. Load schemas

Each database needs a structure export: a CSV file made with one read-only
query from [`export-queries/`](../export-queries). "How to make an export
file" on this screen shows the query and the steps for PostgreSQL, MySQL /
MariaDB, SQL Server, Oracle and SQLite. Details: [import formats](import-formats.md).

- The **first screen** offers three ways in: **Add your exports** (jumps to
  the two panels; *How to make an export file* opens the export steps), the
  **Relational sample** and **Time-series sample** cards, and **Open
  workspace…** to continue a saved comparison.
- Each database has a **panel**: its role (source or target), a state
  (*No export yet*, *Reading…*, *Loaded*), the name, the database engine
  and file, and the number of tables, columns, links and estimated rows.
  Hover over a figure for what it counts.
- **Drag the CSV file onto a panel**, or click the empty area or **Add export
  file** to choose it. Dropping onto a loaded panel replaces its export, like
  **Replace export**. Several files can be dropped or chosen at once (for
  example the four parts of the older PostgreSQL export). A file dropped
  beside the panels is ignored.
- While a file is read, the panel says *Reading <file>…* and its buttons
  wait; other files are not accepted until it has finished.
- A file that is not an export is refused with an **error card** in the
  panel: the file, the reason, which exports are accepted, and **Show me how
  to export**, which opens the export steps. What was loaded before is kept.
  **×** dismisses the card.
- The chips show which parts arrived (columns, keys and links, indexes, row
  counts).
- Click the name to rename the database; the name is used everywhere
  (findings, report, SQL comments).
- The **bridge** between the panels shows the matching result once both are
  loaded. Below it, the summary cards and the *Next* band lead to the next
  stage. *Compare the structure side by side* folds out a table of figures.
- **⇄ Swap** exchanges source and target; choices made by hand are dropped
  because they no longer apply.
- **Open workspace…** reopens a saved comparison.
- A **time-series export** (InfluxDB) shows a *Time series* badge,
  measurements, tags, fields and series instead of tables, columns, links
  and rows, and the retention of each bucket. It is one file; loading another
  replaces it. See [time series](time-series.md).

### 2. Schema checks

Findings about the structure of each schema that can affect the integration,
read from the structure export alone (20 rules, listed in
[rules.md](rules.md)). They are not errors in the running database: Schema
Bridge never connects to it. Low and Info findings about speed, naming,
descriptions and content are notes that can make the integration slower or
harder, not blockers. Every card shows the severity, the database,
the area, the rule id, the object (table, column or link), what it means and
what to do.

- **Severity tiles** are filters: press *High* to see only high findings,
  press again to see all.
- **Search** looks in titles, objects, areas, rule ids, explanations and fixes.
- **Database** buttons narrow to the source or the target.
- **Rule type** narrows to one rule, with the number of findings per rule.
- **Clear filters** shows everything again. Long lists show 100 findings at
  a time with **Show more**.

Severities: **Blocking** (a load will fail or store wrong data), **High**
(affects correctness), **Medium** (needs a decision or a data check), **Low**
(quality, speed, naming), **Info** (context).

### 3. Matching

Which source table goes into which target table, and column by column.
How matches are found is explained in [matching.md](matching.md).

**The table list (left).**

- *Search* finds tables by their name, by the target table's name, and by
  any column name on either side: typing `email` finds every table that has
  an email column.
- *Chips* filter by review state: **Matched**, **No place yet**,
  **Uncertain** (an automatic match with a low score, for the table or one
  of its columns), **Changed by you**, **Stays in source**. Each shows how
  many tables it holds.
- *Sort* by name, by **Needs attention** (tables to check first, uncertain
  ones first) or by **Most columns**.
- Labels under each table: *uncertain*, *changed*.
- With many tables the list shows 200 at a time; use search or chips to
  narrow it.

**The table (right, top).** The pair (source in teal, target in violet), the
reason ("Matched automatically: 76 % similar by table name and column
names") with a confidence bar, and:

- **Target table**: choose another target table, *No place in the target*,
  *Stays in the source*, or *Let the tool decide*.
- **Accept match**: confirms an uncertain automatic match.
- **Reset**: lets the tool decide again.

A red or yellow box lists required target columns that nothing in the source
fills (red when they are links to other tables).

**The columns.** One row per source column:

| Column | Shows |
| --- | --- |
| Source column | Name, type, *key* when it is the primary key, description |
| Target column | A picker to choose any target column, *— not copied —* or *Let the tool decide*; how it was matched (*Automatic · 75 %*, *Word list*, *Changed by you*); target type and description |
| Type conversion | **Same**, **Safe**, **Requires conversion**, **Potential information loss** or **Incompatible**, the two types and a plain explanation ([type compatibility](type-compatibility.md)) |
| Result | copy / convert / check / not copied / never copy, the reason for the match, and notes (empty values, unique values, links) |
| Actions | **✓** accept an uncertain match, **✕** remove the mapping, **↺** undo your change |

Search and chips above the grid narrow the rows (*Mapped*, *Not copied*,
*Uncertain*, *Changed by you*, *Needs a check*). *Draft SQL for this table*
at the bottom shows the statement for this table alone.

**Word list** (second view). Teaches which names mean the same:

```text
component = device                        # a word in the source means a word in the target
limit_min = num_min_value                 # a whole column name (contains _)
table: run = session                      # this table goes into that table ("none" = no place)
column: machine.serial_no = asset.code    # this column goes into that column
skip: timetable*, user_*                  # these tables stay in the source; * matches anything
```

Lines that are not rules are listed under the editor. Changes apply when you
press **Apply word list**; **Load from file…** and **Save to file** exchange
word lists as text files. Your choices on the Tables view always win over the
word list.

**Reset all my choices** forgets every table and column choice.

### 4. Integration issues

Findings about fitting the source into the target, from the matching (9
rules, see [rules.md](rules.md#integration-rules)). They follow your choices:
change a match and the list updates. Search and filters work as on *Schema
checks*.

### 5. Plan

The integration as six steps ([integration plan](integration-plan.md)):

1. Fix the schema findings in the target (its blocking and high findings).
2. Check the data in the source (its blocking and high findings).
3. Make the decisions (blocking and high integration issues, tables with no place).
4. Load the tables in this order (outlined tables already exist and must hold the rows others link to; tables on a cycle are marked with a two-pass suggestion).
5. Run a test load with the draft SQL (with the number of open TODOs).
6. Check the result (row counts, links, a second run, a spot check).

### 6. Draft SQL

The test-load script in load order, or one table at a time. **Copy SQL** and
**Save as .sql** keep it on your computer. Read
[integration-plan.md](integration-plan.md#draft-sql) before running anything
derived from it. For a time-series target this stage shows a notice instead
of SQL ([time series](time-series.md#draft-sql)).

### 7. Report

The comparison for someone who did not run it:

- a verdict: *Ready for a test load* or *N items to resolve before a test load*;
- **Tables**: matched, uncertain, no place yet, stay in source, changed by you;
- **Columns**: same or safe, need conversion, may lose data, incompatible, not mapped;
- **Findings**: blocking, high, medium, low/info;
- **Integration risks** and **Unresolved items** (fix first, decide, review, draft SQL).

**Save report (HTML)** writes a self-contained page that opens in any
browser and prints to PDF; **Save as Markdown** suits wikis and tickets.
Both contain everything listed in [export.md](export.md#comparison-report).

### 8. Export

See [export.md](export.md). In short: the report (HTML, Markdown), findings
(CSV or a table to paste into a spreadsheet), mapping (CSV), draft SQL
(.sql) and the saved comparison (.json).

## Saving and reopening a comparison

**Save comparison** (top bar, or on *Export*) writes a
`….schema-bridge.json` file with both exports, the database names, the word
list and every table and column choice. **Open workspace…** (on *Load* or
*Export*) reopens it exactly as it was. The file stays on your computer and
contains structure only. Nothing is kept in the browser between sessions
otherwise: reloading the page starts empty.

Actions that would throw away work ask first: loading a sample or opening a
saved comparison over your own databases, **Clear**, **⇄ Swap** and **Reset
all my choices** when you have made choices by hand. The question says what
would be removed. Nothing is asked when nothing would be lost, for example
when the untouched samples are replaced. Every saved file and every copy
shows a short confirmation at the bottom right.

## Working with large schemas

- Analysis of 400 × 400 tables takes about a second in a development build
  and is done once per change.
- Use the search boxes: they look into column names, so you rarely need to
  scroll.
- *Sort → Needs attention* puts the tables to check first at the top.
- Lists show 200 tables, 100 findings at a time; **Show more** adds more.
- A word list with table pairs and skip rules removes most of the noise
  from automatic matching in large schemas.

## Progress in the navigation

**Readiness** at the top of the navigation says how close the comparison is
to a first test load, with the same count as the report: *Ready for a test
load*, or *N items to resolve before a test load* (blocking schema findings
and open decisions). The bar shows how many stage checks are clear.

A stage gets a green check only when nothing is open there, not when you
have opened it:

| Stage | Check is clear when |
| --- | --- |
| Load schemas | both databases are loaded |
| Schema checks | no blocking schema finding |
| Matching | no match left to review and no table without a place |
| Integration issues | no blocking or high integration issue |
| Plan | no open decision |
| Draft SQL | no `TODO` left in the draft (no check for a time-series target) |

Report and Export have no check. Under each stage name the navigation says
what is still open ("1 blocking schema finding to fix first", "9 TODOs to resolve"),
or what the stage needs ("Needs both schemas"). Counts show the number of
schema findings, integration issues and TODOs (coloured by the most serious one) and
how many tables are matched.

At the bottom of every stage, **Previous** and **Next** lead to the
neighbouring stages; *Next* says what it needs when it is not available yet.
Changing stage always starts at the top of the new stage.

## Themes and layout

The top bar switches between the **System** colour theme, **Light** and
**Dark**; the choice is remembered the next time you open Schema Bridge. On narrow windows the navigation becomes a strip at the top and the
panels stack.
