# Integration plan and draft SQL

The *Plan* and *Draft SQL* stages turn the comparison into work people can
review. The code is `report::plan_steps`, `plan.rs` (load order) and `sql.rs`
(draft SQL) in `crates/schema-bridge-core/src/`.

## The six steps

| Step | Contains | Why first |
| --- | --- | --- |
| 1. Fix the schema findings in the target | Blocking and high findings of the target database | Good data is rejected or stored wrongly by a broken target |
| 2. Check the data in the source | Blocking and high findings of the source | Missing rules mean bad rows may exist; count them before copying |
| 3. Make the decisions | Blocking and high integration issues, and source tables with no place | The tool cannot decide these; people who know the data must |
| 4. Load the tables in this order | The load order (below) | A table can only be loaded after the tables it links to |
| 5. Run a test load with the draft SQL | The number of open `TODO`s | On a test copy, inside a transaction |
| 6. Check the result | Row counts, links, a second run that must change nothing, a spot check, and unchecked source links to count | Before anyone relies on the data |

## Load order

1. Start with every target table that receives data.
2. Add the tables they link to whenever the link is required or filled from
   the source: those tables must already hold the rows the links point to.
   They are shown outlined (dashed) and are **not** loaded from the source.
3. Sort so every table comes after the tables it links to (topological sort;
   ties by table name).
4. Tables that link to each other in a circle cannot be ordered. They come
   last, marked as a **cycle**; load them in two passes: insert the rows with
   the circular link empty, then fill it with an update.

Example (the sample schemas): `asset_type` (must already exist) → `person ←
operator` → `session ← run` → `asset ← machine` → `capability ←
machine_function`.

## Draft SQL

### What the script contains

```sql
-- 1. Make the Workshop tables (SQLite) readable inside Catalogue (run once, as admin).
CREATE EXTENSION IF NOT EXISTS sqlite_fdw;
CREATE SERVER source_db FOREIGN DATA WRAPPER sqlite_fdw OPTIONS (database '/path/to/file.db');
CREATE SCHEMA IF NOT EXISTS staging;
IMPORT FOREIGN SCHEMA main LIMIT TO (machine, …) FROM SERVER source_db INTO staging;

-- 2. Load in this order, on the TEST database, inside one transaction
BEGIN;
-- machine → asset
-- not copied: room, power_kw, notes
INSERT INTO public.asset (uuid, name, code, asset_type_id, add_info)
SELECT
  NULL /* TODO: id: integer and uuid do not fit together. */,
  s.label,
  s.serial_no,
  NULL,  -- TODO: required link to asset_type, no source column
  jsonb_build_object('source_system', 'Workshop', 'source_table', 'machine', 'source_id', s.id)  -- keeps the original ID
FROM staging.machine s;

-- 3. Check the results (counts, links), then:
-- COMMIT;   or   ROLLBACK;
```

Per target table, in load order:

- one `INSERT … SELECT` line per mapped column;
- **casts** where the type needs a conversion, with the reason as a comment;
- **lookups** (a sub-select by name) where text must become the ID of
  another record, marked `TODO: check the match column`;
- times without a time zone converted with `AT TIME ZONE '<source time zone>'`
  and a `TODO` (the placeholder fails until someone fills in the real zone);
- `NULL -- TODO` for required columns nothing fills, and for incompatible types;
- the source system, table and ID kept in a JSON column when the target has
  an unused one (`add_info`, `extra`, `meta`, …);
- a comment listing source columns that are not copied.

The staging part uses a PostgreSQL **foreign data wrapper** for the source's
system (`postgres_fdw`, `mysql_fdw`, `tds_fdw`, `oracle_fdw`, `sqlite_fdw`)
with placeholder connection options. For a system without a known wrapper
the script says to export each table to CSV and load it with `COPY` into the
`staging` schema.

### Safety

- Schema Bridge **never runs** the script and has no database connection to
  run it with. Copy and Save only put text on your computer.
- The script opens a transaction and **leaves `COMMIT` commented out**, so
  running it as it is changes nothing permanently.
- Connection options are **placeholders** (`'<host>'`, `'<password>'`), never
  real values.
- Every open decision is a `TODO`; the *Plan* shows how many remain.

### Limitations

- Written in **PostgreSQL syntax** (specification FR-16). For another target
  the script starts with a note and serves as a column-by-column guide.
- For a source system **without a known foreign data wrapper** (InfluxDB,
  or an unknown system) the script creates CSV staging tables and writes a
  `\copy` command per table instead of `IMPORT FOREIGN SCHEMA`.
- For a **time-series target** (InfluxDB) **no SQL is generated**: such
  databases are not written with SQL `INSERT`s, and Schema Bridge does not
  write line protocol. Step 5 of the plan, the Draft SQL stage, the Export
  card and the report say so ([time series](time-series.md#draft-sql)).
- Lookups match by a column called `name`; check the match column.
- There is no upsert mode; running the load twice duplicates rows unless the
  target rejects them. Step 6 checks this.
- It does not handle cycles automatically (two passes are suggested).
- Review it with someone who knows both databases, and run it on a test copy
  first.
