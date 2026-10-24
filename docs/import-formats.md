# Import formats

Schema Bridge reads **structure exports**: CSV files that describe a
database's tables, columns, keys, links and indexes. You make them yourself
with one read-only query, so Schema Bridge never needs a connection or a
password. Two formats are accepted.

## The one-file export (recommended)

One CSV file with 21 columns. The `record_type` column says what a row
describes:

| `record_type` | Describes | Columns it uses |
| --- | --- | --- |
| `column` | One column of a table or view | `table_schema`, `table_name`, `object_type` (`table`, `view`, …), `ordinal_position`, `column_name`, `data_type`, `is_nullable`, `column_default`, `table_comment`, `column_comment` |
| `constraint` | A primary key, unique rule, link (foreign key) or check | `constraint_name`, `constraint_type` (`PRIMARY KEY`, `UNIQUE`, `FOREIGN KEY`, `CHECK`), `definition` (for example `FOREIGN KEY (a) REFERENCES s.t(b)`, with `NOT VALID` when existing rows were never checked), `ref_schema`, `ref_table` |
| `index` | An index | `indexname`, `indexdef` (`CREATE [UNIQUE] INDEX … ON … (columns) [WHERE …]`) |
| `table` | Size and triggers | `estimated_rows`, `triggers` |

Every row also carries `dbms`: `postgresql`, `mysql`, `sqlserver`, `oracle` or
`sqlite`. The header row is:

```text
record_type,table_schema,table_name,object_type,ordinal_position,column_name,data_type,is_nullable,column_default,table_comment,column_comment,constraint_name,constraint_type,definition,ref_schema,ref_table,indexname,indexdef,estimated_rows,triggers,dbms
```

Tolerated variations: files saved **without the header row**, `;` as the
separator (common in European Excel settings), files saved on Windows
(`\r\n`), and a byte-order mark. An unquoted empty field or `NULL` means "no
value"; a quoted `"NULL"` is the text NULL.

### Making the export

| System | Query | Where to run it | Tested on a real server |
| --- | --- | --- | --- |
| PostgreSQL | [`postgresql.sql`](../export-queries/postgresql.sql) | pgAdmin Query Tool, psql, DBeaver | Yes (16) |
| MySQL / MariaDB | [`mysql.sql`](../export-queries/mysql.sql) | MySQL Workbench, DBeaver, phpMyAdmin (`USE your_database;` first) | Not yet |
| SQL Server 2017+ / Azure SQL | [`sqlserver.sql`](../export-queries/sqlserver.sql) | SSMS (turn on "Include column headers" and "Quote strings containing list separators" first), Azure Data Studio | Not yet |
| Oracle 12c+ | [`oracle.sql`](../export-queries/oracle.sql) | SQL Developer, as the schema owner | Not yet |
| SQLite 3.16+ | [`sqlite.sql`](../export-queries/sqlite.sql) | DB Browser for SQLite, or the `sqlite3` shell | Yes (3.45) |

The same steps appear in the application under *Load schemas → How to make
an export file*, with a copy button for each query.

Example with the command-line tools:

```sh
# PostgreSQL
psql -d mydb -c "\copy ($(sed 's/--.*$//;s/;\s*$//' export-queries/postgresql.sql | tr '\n' ' ')) TO 'mydb.csv' CSV HEADER"

# SQLite
sqlite3 -header -csv mydb.sqlite < export-queries/sqlite.sql > mydb.csv
```

Every query only reads the system catalogue (a test checks that none of them
contains a statement that writes), and none reads table contents.

## The four-file export (PostgreSQL, older)

Four CSV files, one per query, as earlier versions produced them:
`columns.csv`, `constraints.csv`, `indexes.csv`, `rowcounts.csv`. Add them one
after another to the same database; each is recognised by its header. The
sample [`demo/catalogue-postgresql-4files/`](../demo/catalogue-postgresql-4files)
shows the layout. Both formats give the same result (a test checks this).

## What is read, and what is not

Read: tables (views are recognised but only listed), columns with their type,
nullability, default and description, primary keys, unique rules, links with
their delete rule and whether they were validated, check constraint
definitions, indexes (unique, partial), estimated row counts and trigger
names.

Not read or not compared: view definitions, procedures and functions,
trigger bodies, check constraint contents, permissions, partitioning,
sequences, and table data.

Tables named `databasechangelog*` (Liquibase bookkeeping) are ignored.

## Type names

The original type text is kept for display (`NVARCHAR(50)`), and a
normalised form is used for comparison. Three names mean something different
in one system and are translated at import: Oracle `DATE` (includes a time),
SQL Server `timestamp`/`rowversion` (a binary row version) and MySQL `float`
(4 bytes). See [type compatibility](type-compatibility.md).

## The saved comparison file

Not an import format for a database, but read on the same screen: see
[export.md](export.md#saved-comparison).
