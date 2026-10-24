# Type compatibility

For every mapped column, Schema Bridge judges whether the source values fit
into the target column, and explains it in plain words. The result appears
in the Matching grid (*Type conversion*), in the report, in the mapping CSV
and in the draft SQL. The code is `compatibility()` in
`crates/schema-bridge-core/src/types.rs`.

## The five results

| Result | What it means for the data | What the draft SQL does |
| --- | --- | --- |
| **Same** | Both columns have the same type; values are copied unchanged. | copies the value |
| **Safe** | Every value fits without losing anything (a smaller number into a bigger one, short text into longer text). | copies the value |
| **Requires conversion** | Values must be converted or looked up on the way. | writes a cast, a lookup, or a time-zone conversion, marked `TODO` where a decision is needed |
| **Potential information loss** | Some values may be cut, rounded or lose detail. Check the data first. | writes a cast with the explanation as a comment |
| **Incompatible** | Cannot be converted automatically; someone must decide how to fill the column. | writes `NULL /* TODO … */` |

## Examples

| Source → target | Result | Explanation shown |
| --- | --- | --- |
| `integer` → `integer` | Same | Same type |
| `integer` → `bigint` | Safe | Whole number fits |
| `varchar(20)` → `text` | Safe | Text fits |
| `integer` → `numeric(10,2)` | Safe | Whole number into decimal |
| `date` → `timestamp` | Safe | Date becomes midnight |
| `timestamp` → `timestamptz` | Requires conversion | Source time has no time zone. Say which time zone it was recorded in. |
| `text` → `uuid` (a link) | Requires conversion | Source has text, target expects the ID of another record. Each text value must be looked up and replaced by that ID. |
| `text` → `integer` | Requires conversion | Every value must be a valid integer. Test with a query first. |
| `boolean` → `integer` | Requires conversion | true/false becomes 1/0. |
| `real` → `text` | Requires conversion | Saved as text. Works, but the target can no longer check the value. |
| `varchar(100)` → `varchar(20)` | Potential information loss | Target allows 20 characters, source allows 100. Longer values would be rejected. |
| `bigint` → `smallint` | Potential information loss | Target number range is smaller. |
| `numeric` → `integer` | Potential information loss | Decimals would be rounded off. |
| `timestamp` → `date` | Potential information loss | Time of day is dropped. |
| `timestamptz` → `timestamp` | Potential information loss | Time zone is dropped. |
| `integer` → `uuid` | Incompatible | integer and uuid do not fit together. |
| `bytea` → `integer` | Incompatible | Binary data cannot be converted automatically. |
| `integer[]` → `integer` | Incompatible | Source holds a list of values, target holds one. Needs splitting or a link table. |

## How types are recognised

Each type name is put into a category (yes/no, text, whole number, decimal,
exact numeric, timestamp, date, time, UUID, JSON, binary, XML, interval, or
other) with a size: bytes for numbers, characters for text (unlimited for
`text`), and for timestamps whether a time zone is stored. Names from all
five supported systems are recognised: `nvarchar(50)`, `NUMBER(10,0)` (a
whole number by its precision), `tinyint(1)` and `bit` (yes/no), Oracle
`DATE` (with time), SQL Server `datetimeoffset` (with time zone), SQLite's
free-form names by affinity. Unknown names fall into *other* and are
reported as "Special type …: check the allowed values".

## Origin and tests

Ported from the original prototype and tested against its answers for 94
type names and 65 type pairs, with two deliberate differences: a list never
fits a single value, and the time-zone note does not suggest a specific zone
([decision 12](decisions.md)).
