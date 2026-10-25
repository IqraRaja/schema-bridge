# Time-series schemas

Schema Bridge can compare a time-series schema (InfluxDB first) with a
relational one or with another time-series schema. It uses the same workflow,
screens and neutral model as relational databases. This page explains what is
read, how the time-series concepts are treated, which problems are reported
and what Schema Bridge does not do.

The design decision is [decision 24](decisions.md#24-time-series-schemas-in-the-neutral-model-influxdb-first).

## Concepts

| InfluxDB | Meaning | In Schema Bridge |
| --- | --- | --- |
| Bucket (1.x: database/retention policy) | Holds measurements and keeps them for a retention period | A retention policy of the schema; each measurement names its bucket |
| Retention period | How long points are kept (`30d`, `INF`) | Shown on the database panel and in the report; never converted |
| Measurement | A named set of points | A table of kind *measurement* |
| `time` | The point's time, in nanoseconds | A column with the role **time**, type `timestamp (ns)`, required |
| Tag | Indexed text that identifies a series (`site`, `unit_id`) | A column with the role **tag**, type `string` |
| Field | A measured value with a type (`temperature`) | A column with the role **field** and its type |
| Series | One combination of tag values | The measurement's series count (cardinality), not a row count |

A point is identified by its measurement, tag values and time. Writing a
second point with the same identity silently replaces the first. InfluxDB has
no primary keys, links, constraints or explicit indexes. Schema Bridge
therefore runs no relational key, link or index rules on measurements. It
also never turns tags + time into a primary key; when that identity matters,
it asks you to review it.

## The time-series schema export

Schema Bridge reads its **own normalised interchange format**: a CSV file that
describes a bucket. It is *not* a native InfluxDB format. You can produce it
with the provided Flux script or SQL query, by hand from InfluxQL output, or
with any other tool.

```
record_type,bucket,retention,measurement,key,data_type,series,dbms
bucket,plant,30d,,,,,influxdb
measurement,plant,,boiler,,,412,influxdb
tag,plant,,boiler,site,,,influxdb
field,plant,,boiler,temperature,float,,influxdb
```

| Record type | Columns used |
| --- | --- |
| `bucket` | `bucket`, `retention` (`30d`, `720h`, `90m`, `3600s`, `2w`, or `INF`/`0`/`infinite` for "kept forever") |
| `measurement` | `bucket`, `measurement`, optionally `series` (the number of series) |
| `tag` | `bucket`, `measurement`, `key` |
| `field` | `bucket`, `measurement`, `key`, `data_type` (`float`, `integer`, `unsigned`, `string`, `boolean`, or empty when unknown) |

Rules:

- **The file is recognised by its header.** It needs `record_type`,
  `measurement` and `key`, and no `table_name`. Column order does not matter.
- **Flux annotated CSV is accepted unchanged.** Comment rows starting with
  `#`, the extra `result` and `table` columns, and repeated headers are all
  allowed.
- **A field may appear more than once with different types** (InfluxDB keeps
  types per shard). Schema Bridge keeps every type as a conflict, for example
  `integer | float`.
- **An empty field type stays unknown.** It is reported, not guessed.
- **A field with the same key as a tag** is kept as `key_1` and reported.
- **Bucket rows are optional.** Without them the retention is unknown.
- **Mistakes are refused with their line number:** an unknown record type, a
  missing name or key, an unknown field type, a retention or series count
  that is not a number, or a tag, field or measurement without its
  measurement or bucket row.

Loading a new time-series export into a database replaces what was loaded
before; it is a single-file format.

### Producing the export

| Source | How |
| --- | --- |
| InfluxDB 2.x, and 1.8 with Flux | [`export-queries/influxdb.flux`](../export-queries/influxdb.flux): set the bucket name, run it in the Data Explorer or with `influx query`, save the result as CSV. Field types need InfluxDB 2.2 or newer (`types` package) |
| InfluxDB 3 | [`export-queries/influxdb3.sql`](../export-queries/influxdb3.sql): reads `information_schema`; retention and series counts are not available there and stay empty |
| InfluxDB 1.x (InfluxQL) | Run `SHOW RETENTION POLICIES`, `SHOW MEASUREMENTS`, `SHOW TAG KEYS`, `SHOW FIELD KEYS` and `SHOW SERIES EXACT CARDINALITY`, then write the rows in the format above. Use `database/retention_policy` as the bucket name, for example `telegraf/autogen` |

All of these only read. **Neither recipe has been run on a real server yet**
(the load screen says so). They read metadata and, for Flux, the last point
of each series; they never write.

## Problems inside a time-series schema

These rules run only on measurements (area *Time series*):

| Rule id | Severity | Finds |
| --- | --- | --- |
| `TS-MEASUREMENT-NO-FIELDS` | High | A measurement with no fields in the export |
| `TS-FIELD-TYPE-CONFLICT` | High | A field with more than one type; a typed target column accepts only one |
| `TS-FIELD-TYPE-UNKNOWN` | Medium | A field whose type is not in the export |
| `TS-TAG-FIELD-SAME-KEY` | Medium | A field that shares its key with a tag; queries and exports must tell them apart |
| `TS-HIGH-CARDINALITY` | Medium / Low | Medium: 100,000 series or more. Low: a tag whose name suggests unbounded values (`uuid`, `request_id`, `session_id`, `trace_id`, `span_id`) |
| `TS-UNSIGNED-FIELD` | Info | An unsigned integer field; most relational systems have no unsigned 64-bit type |

Relational tables in the same comparison are checked with the relational rules
as before.

## Matching

Measurements are matched like tables and their columns like any other
column: by names, the word list, descriptions and types. InfluxDB calls its
time column `time`, so a word-list rule such as `time = recorded_at` is often
needed. Nothing is forced: a measurement with no suitable table gets *no place
yet*.

The type fit takes the time-series facts into account:

- **Time precision.** `time` (nanoseconds) into a relational time column is
  *potential information loss*. PostgreSQL and MySQL keep microseconds, SQL
  Server `datetime2` 100 ns, and others keep milliseconds or less. Time into
  time between two time-series schemas is *same*.
- **Unknown field types** give *requires conversion*. The note says the type
  is not in the export.
- **Type conflicts** give *potential information loss*. The note names the
  types.
- **Role changes.** A tag that becomes a field (or the reverse) between two
  time-series schemas is noted on the column and reported as
  `MAP-TS-ROLE-CHANGE`.

## Integration issues

| Rule id | Severity | Finds |
| --- | --- | --- |
| `MAP-TS-SERIES-KEY` | Medium | **Measurement → table:** the target has no unique rule on the columns that receive the tags and time. Loading overlapping exports twice would store duplicate rows. Schema Bridge does not add the rule; review whether it fits. **Table → measurement:** the source has no unique rule on the columns that become tags and time, so rows would overwrite each other |
| `MAP-TS-RETENTION` | Medium | A source with a finite retention into a relational target. Only that period exists to migrate, and the target keeps rows until they are deleted (PostgreSQL has no retention period) |
| `MAP-TS-RETENTION` | High | A time-series target that keeps data for a shorter time than the source. Older points are dropped when written |
| `MAP-TS-ROLE-CHANGE` | Medium | A tag that becomes a field, or a field that becomes a tag |

`MAP-NO-SOURCE-ID` is not reported for measurements. A point has no ID; its
identity is tags + time, which `MAP-TS-SERIES-KEY` covers.

## Draft SQL

- **InfluxDB → PostgreSQL:** the usual draft, with **CSV staging**. No
  foreign data wrapper is assumed. The draft creates one `staging` table per
  measurement (time first, then tags and fields; unknown or conflicting
  types as `text`) and writes a `\copy … FROM '<measurement>.csv'` command
  for you to run from `psql`. Then come the `INSERT … SELECT` statements in
  load order. Export each measurement's points to CSV with the database's
  own tools.
- **PostgreSQL (or any system) → InfluxDB:** **no SQL is generated.**
  Time-series databases are not written with SQL `INSERT` statements, and
  Schema Bridge does not generate InfluxDB line protocol or other write
  operations. The Plan, Draft SQL stage, Export card and report say so.
  Use the mapping (Matching stage, or the mapping CSV on Export) with the
  database's own tools.

## The sample

The *Time-series sample* card on the first screen loads two made-up
databases:

- **Plant** ([`demo/plant-influxdb.csv`](../demo/plant-influxdb.csv)): bucket
  `plant`, kept 30 days, with three measurements:
  - `boiler`: 412 series; tags `site`, `unit_id`; fields `temperature`,
    `pressure`, `running` and `firmware_version` (type unknown);
  - `pump`: 186 series; `speed_rpm` is both integer and float;
  - `energy_meter`: 24 series; `total_kwh` is unsigned.
- **Readings** ([`demo/readings-postgresql.csv`](../demo/readings-postgresql.csv),
  DDL in [`demo/sql/readings-postgresql.sql`](../demo/sql/readings-postgresql.sql),
  written by hand): `readings` (id identity, `recorded_at`, `site`, `unit_id`,
  `temperature`, `pressure`, `running`) with no unique rule on the point
  identity, and `pump_readings` with `UNIQUE (site, unit_id, recorded_at)`.
- **Word list** ([`demo/sample-timeseries-word-list.txt`](../demo/sample-timeseries-word-list.txt)):
  `time = recorded_at` and `table: boiler = readings`.

What you see (checked by `crates/schema-bridge-core/tests/timeseries.rs`):

- **Load:** a *Time series* badge; 3 measurements, 6 tags, 9 fields and 622
  series; *Retention plant: 30 days*.
- **Problems:** in Plant, `TS-FIELD-TYPE-CONFLICT` (pump.speed_rpm),
  `TS-FIELD-TYPE-UNKNOWN` (boiler.firmware_version) and `TS-UNSIGNED-FIELD`
  (energy_meter.total_kwh). No relational rules fire.
- **Matching:**
  - `boiler → readings` comes from the word list; `pump → pump_readings` is
    automatic; `energy_meter` has no place.
  - `time → recorded_at` is potential information loss (nanoseconds).
  - `firmware_version` is not copied.
- **Integration issues:**
  - `MAP-TS-SERIES-KEY` on boiler → readings, but not on pump → pump_readings,
    which has the unique rule;
  - `MAP-TS-RETENTION` on both mappings;
  - the usual type-fit and empty-value findings.
- **Draft SQL:** CSV staging for `boiler` and `pump`, then the inserts.

Swap the two databases to see the time-series target: the Draft SQL stage
shows the notice instead of SQL.

## Limitations

- Only the schema export is read: never points, never a live server.
- Series counts come from the export. A series count is not a row count; the
  number of points is unknown.
- Retention is shown as exported. Shard-group durations, downsampling tasks
  and continuous queries are not read.
- One time-series system so far (InfluxDB 1.x/2.x/3 through the interchange
  format). Prometheus, OpenTelemetry and other time-series databases, a JSON
  form of the export, direct connections and line-protocol output are not
  implemented. The importer boundary and model leave room for them; see
  [adding a database](adding-a-database.md#case-2-a-system-that-does-not-fit-the-one-file-export).
