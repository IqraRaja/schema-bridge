# Database systems

Schema Bridge is designed to be database-neutral: importers turn a
system-specific export into one neutral model, and checks, matching,
integration planning and the report work on that model only. This page says
exactly what works today. Nothing here is a promise beyond the current
release.

## 1. Systems whose types are recognised

Type names from these systems are recognised and compared across systems
(94 names in the test set):

| System | Examples of recognised names |
| --- | --- |
| PostgreSQL | `integer`, `bigint`, `numeric(10,2)`, `text`, `varchar(100)`, `timestamp`, `timestamptz`, `uuid`, `jsonb`, `bytea`, arrays (`integer[]`) |
| MySQL / MariaDB | `int unsigned`, `tinyint(1)` (yes/no), `varchar`, `datetime`, `json`, `blob`, `float` (4 bytes) |
| SQL Server | `nvarchar(50)`, `bit`, `datetime2`, `datetimeoffset`, `uniqueidentifier`, `timestamp`/`rowversion` (binary) |
| Oracle | `NUMBER(10,0)`, `VARCHAR2`, `DATE` (with a time), `CLOB`, `RAW` |
| SQLite | free-form names by SQLite's affinity rules (`INTEGER`, `TEXT`, `REAL`, `DATETIME`) |

Unknown names are treated as "other" and compared by name only.

## 2. Systems whose schemas can be imported

Through the one-file CSV export ([import formats](import-formats.md)):

| System | Export query | Query run on a real server |
| --- | --- | --- |
| PostgreSQL | yes (plus the older four-file export) | yes, PostgreSQL 16 |
| SQLite | yes | yes, SQLite 3.45 |
| MySQL / MariaDB | yes, one query for both | not yet |
| SQL Server 2017+ / Azure SQL | yes | not yet |
| Oracle 12c+ | yes | not yet |

**Other relational systems** (Db2, SAP HANA, Snowflake, …) can be loaded if
you write a query that produces the same 21 columns. Schema Bridge then shows
them by their system code and analyses them, but their type names are not
specifically recognised, system-specific advice (for example how to validate
a link) falls back to neutral wording, and the draft SQL uses CSV staging.
See [adding a database](adding-a-database.md) to add proper support.

There are **no direct database connections**: every schema comes from a
file you export yourself.

## 3. Planned adapters

In the development plan, not implemented:

- **Time-series databases**, starting with InfluxDB / InfluxDB-style schemas
  (see below), then other time-series systems.
- A **separate MariaDB** entry where MariaDB differs from MySQL.
- Running the MySQL, SQL Server and Oracle queries on real servers and fixing
  what they show.
- Further relational systems through the same extension point.

## 4. Database features that are not compared

Even for the supported systems:

- **Views**: their names are read; their columns and definitions are not
  analysed or matched.
- **Triggers, procedures, functions**: trigger names are read, nothing is compared.
- **Check constraints**: read, but their contents are not compared.
- **Generated columns, sequences, partitioning, permissions, collations**: not read.
- **Data**: never read. Findings are risks seen in the structure.

## Time-series databases

Time-series databases organise data differently from relational ones:

| Concept | Meaning | Relational nearest thing |
| --- | --- | --- |
| Measurement | A named series of points (InfluxDB "measurement", Prometheus metric) | A table, but without keys or links |
| Timestamp | Every point's time; the primary axis | A column, but special |
| Tag / label | Indexed metadata that identifies a series (`host`, `region`) | A column, but indexed and part of the identity |
| Field | A measured value (`usage`, `temperature`) | A column |
| Retention policy | How long data is kept (`30d`, `INF`) | No equivalent |

Forcing these into tables would lose meaning (a tag is not just a text
column; a retention policy is not a column at all). The neutral model
therefore has **extension points** for them:

- a table's `kind`: `Table` or `Measurement`;
- a column's `role`: `Regular`, `Timestamp`, `Tag` or `Field`;
- a measurement's `retention_policy`, and the schema's list of
  `retention_policies`.

**What exists today:** only these model fields, with a test that a
measurement passes through the analysis without failing. **What does not
exist yet:** an importer for any time-series system, checks and matching
rules that use tags, fields and retention policies, and draft SQL for a
time-series target. Until then, Schema Bridge cannot import or meaningfully
compare a time-series schema.

The order of work is in the [development plan](development-plan.md)
(milestone 8) and in [adding a database](adding-a-database.md#case-2-a-system-that-does-not-fit-the-one-file-export).
