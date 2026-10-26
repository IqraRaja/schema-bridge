# Sample data

Two small, made-up databases for trying Schema Bridge without access to a real
database. They are ordinary exports: Schema Bridge reads them exactly as it
reads your own files. They contain structure only (tables, columns, keys, indexes), no rows
of real data, and no credentials.

| File | Database | Role |
| --- | --- | --- |
| `workshop-sqlite.csv` | **Workshop** (SQLite) | Source: the database you want to bring in |
| `catalogue-postgresql.csv` | **Catalogue** (PostgreSQL) | Target: the database that receives it |
| `catalogue-postgresql-4files/` | Catalogue again | The same target in the older four-file export format |
| `sql/*.sql` | Both | The SQL that creates the two databases |
| `sample-word-list.txt` | Both | The word list loaded with the samples: which Workshop names mean which Catalogue names |
| `plant-influxdb.csv` | **Plant** (InfluxDB) | Time-series sample, source: a bucket with three measurements, in the [time-series schema export](../docs/time-series.md#the-time-series-schema-export) |
| `readings-postgresql.csv` | **Readings** (PostgreSQL) | Time-series sample, target; written by hand (DDL in `sql/readings-postgresql.sql`) |
| `sample-timeseries-word-list.txt` | Plant → Readings | `time = recorded_at`, `table: boiler = readings` |

The time-series sample and what it shows are described in
[docs/time-series.md](../docs/time-series.md#the-sample).

## The scenario

A workshop runs machines and records each run in its own control database
(**Workshop**). An organisation-wide catalogue (**Catalogue**) should now
describe those machines, people and runs. The question Schema Bridge answers
is: what fits, what does not, and what has to be decided before the workshop
data can be loaded into the catalogue?

| Workshop table | Fits into Catalogue table | Note |
| --- | --- | --- |
| `machine` | `asset` | The catalogue also needs an asset type, which the workshop does not record |
| `machine_function` | `capability` | The workshop links functions to one machine; the catalogue to an asset *type* |
| `run` | `session` | |
| `operator` | `person` | The workshop stores one full name; the catalogue needs first and last name |
| `booking` | none | Operational data that stays in the workshop |

## Deliberate flaws

The demo contains schema flaws on purpose, so every kind of finding has an
example.

In **Workshop**:

- `machine.serial_no` identifies a machine but has no unique rule.
- `machine_function.machine_id` looks like a link but has no foreign key.
- `run.station_nr` and `booking.station_nr` are plain numbers with no list of valid stations, and default to `0`.
- `run.settings_xml` and `run.settings_json` store the same settings twice.

In **Catalogue**:

- `asset.parent_asset_id` points to `location` instead of `asset`, and the
  link was added with `NOT VALID`, so existing rows were never checked.
- `asset.start` is stored without a time zone, `asset."end"` with one.
- `person.orcid` identifies a person but has no unique rule.

All of these are reported on the **Schema checks** screen, together with
smaller findings such as links without an index. Issues in fitting the two
databases together (for example the missing asset type, or the operator's
single full name against first and last name) are on the **Integration
issues** screen.

## Recreating the files

The CSV files are the output of the export queries in
[`export-queries/`](../export-queries) run on databases created by `sql/`.
The commands are at the top of each SQL file.
