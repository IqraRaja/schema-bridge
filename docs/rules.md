# Problem detection rules

Schema Bridge checks each database on its own (20 rules), and then how the
two fit together (9 rules). Every finding has a rule id, a severity, the
object it is about, a plain-language explanation and what to do. All rules
work on the structure alone; none reads data. The code is in
`crates/schema-bridge-core/src/checks.rs` and `integration.rs`.

## Severities

| Severity | Meaning |
| --- | --- |
| **Blocking** | A load will fail or store wrong data until this is solved. |
| **High** | Affects correctness; solve before relying on the data. |
| **Medium** | Needs a decision or a check of the data. |
| **Low** | Quality, speed or naming. |
| **Info** | Context; nothing is wrong. |

## Rules inside one database

| Rule id | Severity | Area | Finds |
| --- | --- | --- | --- |
| `LINK-WRONG-TARGET` | Blocking | Links | A link whose column name points to a different table than the one it references (`parent_asset_id` → `location`) |
| `KEY-MISSING-PK` | High | Keys | A table without a primary key |
| `LINK-NOT-VALIDATED` | High | Links | A link added with `NOT VALID` (or untrusted/disabled): older rows were never checked. The fix includes the system's validation statement for PostgreSQL, SQL Server and Oracle |
| `SEC-SENSITIVE` | High | Security | A column that looks like a password, token or secret |
| `LINK-IMPLICIT` | Medium | Links | A column that looks like a link (`*_id`, `uuid_*`, a UUID type) but nothing checks it |
| `LINK-SHARED-CODE` | Medium | Links | The same code column (`*_nr`, `*_code`, `*_type`) in several tables without a lookup table |
| `DUP-IDENTIFIER` | Medium | Duplicates | An identifier-like column (DOI, ORCID, e-mail, serial number, …) without a unique rule |
| `KEY-SURROGATE-NOT-UNIQUE` | Medium | Keys | An extra id column beside a composite primary key that is not unique |
| `TIME-MIXED-TZ` | Medium | Time | Time columns with and without time zone in the same table |
| `DATA-MULTI-FORMAT` | Medium | Consistency | The same data stored in several formats (`*_xml`, `*_json`, …) |
| `LINK-ID-ARRAY` | Low | Links | A list of IDs that the database cannot check |
| `DUP-NULLABLE-UNIQUE` | Low | Duplicates | A unique column that may be empty |
| `PERF-FK-NO-INDEX` | Low | Speed | A link column without a supporting index |
| `PERF-DUP-INDEX` | Low | Speed | Two indexes on the same columns |
| `NAME-RESERVED` | Low | Naming | A name that is an SQL reserved word |
| `NAME-MISLEADING` | Low | Naming | A constraint name that contradicts its type (`fk_` on a primary key) |
| `NAME-LINK-COLUMN` | Low | Naming | A link column whose name suggests a related table it does not reference |
| `KEY-MIXED-GENERATORS` | Low | Keys | Different ID generator functions across tables |
| `DOC-COVERAGE` | Low or Info | Descriptions | Share of columns with a description (Low below 30 %); not checked for SQLite, which has no descriptions |
| `CONTENT-EMPTY` | Info | Content | Tables with no rows, or whose statistics were never collected |

Rules that need a part of the export that is missing (for example indexes)
are skipped rather than reporting false problems.

### Time-series rules

These run only on measurements (time-series schemas); the rules above run
only on tables, so a measurement is never reported for a missing primary key
or an unchecked link. Details and examples: [time series](time-series.md).

| Rule id | Severity | Area | Finds |
| --- | --- | --- | --- |
| `TS-MEASUREMENT-NO-FIELDS` | High | Time series | A measurement with no fields in the export |
| `TS-FIELD-TYPE-CONFLICT` | High | Time series | A field with more than one type (`integer \| float`) |
| `TS-FIELD-TYPE-UNKNOWN` | Medium | Time series | A field whose type is not in the export |
| `TS-TAG-FIELD-SAME-KEY` | Medium | Time series | A field that shares its key with a tag |
| `TS-HIGH-CARDINALITY` | Medium or Low | Time series | Medium: 100,000 series or more; Low: a tag named like an unbounded value (`uuid`, `request_id`, `session_id`, `trace_id`, `span_id`) |
| `TS-UNSIGNED-FIELD` | Info | Time series | An unsigned integer field |

## Integration rules

These compare the two databases through the matching, so they change when
you change a match.

| Rule id | Severity | Finds |
| --- | --- | --- |
| `MAP-REQUIRED-LINK` | Blocking | A required target column that links to another table, and nothing in the source fills it |
| `MAP-REQUIRED-VALUE` | High | A required target column (no default) that nothing in the source fills |
| `MAP-RELATIONSHIP` | High | A source link that lands on a different table than the matching target link |
| `MAP-DUPLICATES` | High | A unique target column fed from a source column without a unique rule |
| `MAP-TYPE-FIT` | Medium | A mapped column whose values may be cut, rounded, lose their time zone, or cannot be converted |
| `MAP-EMPTY-VALUES` | Medium | Source columns that may be empty mapped to required target columns |
| `MAP-NO-SOURCE-ID` | Medium | No place in the target to keep the original source ID, so a load cannot be repeated or traced (not for measurements, whose points have no ID) |
| `MAP-NO-TARGET` | Medium | A source table with no place in the target |
| `MAP-MANY-TO-ONE` | Info | Several source tables go into one target table |
| `MAP-TS-SERIES-KEY` | Medium | Measurement → table: no unique rule on the columns that receive tags and time (not added automatically). Table → measurement: rows that share tag and time values would overwrite each other |
| `MAP-TS-RETENTION` | Medium or High | Medium: a finite source retention into a relational target, which has no retention period. High: a time-series target that keeps data for a shorter time |
| `MAP-TS-ROLE-CHANGE` | Medium | A tag that becomes a field, or the reverse |

## Filtering

On the *Problems* and *Integration issues* screens, findings can be searched
(title, object, area, rule, explanation, fix) and filtered by database,
severity and rule id. See the [user guide](user-guide.md#2-problems).

## Origin and testing

The rules are a port of the original browser prototype's checks, with the
same wording; parity tests compare their output with the prototype's on the
sample schemas and on a coverage fixture that triggers every rule
([decision 15](decisions.md)). Rule ids follow the specification's catalogue
(§7); `NAME-LINK-COLUMN`, `KEY-MIXED-GENERATORS` and the `MAP-*` ids were
added because the catalogue does not list them.
