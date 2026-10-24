# Matching methodology

How Schema Bridge decides which source table goes into which target table,
and which column into which column. The code is in
`crates/schema-bridge-core/src/matching.rs`; the user-facing screen is
described in the [user guide](user-guide.md#3-matching).

## 1. Names become words

Every name is split into lower-case words: `snake_case` and `camelCase`
both work (`componentFunctions` → `component`, `function`). Then:

- a plural `s` is dropped from words longer than four letters (`devices` →
  `device`, but `bus` and `address` stay);
- `id`, `uuid` and `pk` count as the same word;
- each word is translated through the **word list** (`component = device`).

Column names are lower-cased before they are split (as in the original
prototype), so camelCase only splits table names.

## 2. Similarity

Two word sets are compared with the **Dice coefficient**: twice the number of
shared words divided by the total number of distinct words. `machine_id` and
`asset_type_id` with `machine = asset` in the word list: {asset, id} and
{asset, type, id} share 2 words of 5, so 2 × 2 / 5 = 0.8.

Two column names are 100 % similar when they are equal (ignoring case) or the
word list pairs the whole names (`limit_min = num_min_value`).

## 3. Tables

For each source table, every target table gets a score:

```text
score = 0.6 × name similarity + 0.4 × share of source columns (not the primary key)
                                   that have a target column at least 80 % similar
```

The same table name scores 1. Then, in order of strength:

1. **Your choice** on the Matching screen (a target table, *no place*, or *stays in the source*).
2. A **table pair** in the word list (`table: run = session`; `none` means no place).
3. A **skip rule** in the word list (`skip: user_*`): the table stays in the source.
4. The **best automatic match**, if its score is at least **0.55**.

Otherwise the table has no place yet; the closest candidate (score of at
least 0.3) is still shown.

## 4. Columns

For each matched table pair, every source/target column pair with a name
similarity of at least 0.5 gets a score:

```text
score = name similarity
      + 0.1 × description similarity (meaningful words of the two descriptions)
      + 0.1 if the types fit (same or safe), −0.25 if they are incompatible
```

A target column that is a **link** is never filled automatically from a
plain source column (only from a source link or the primary key): such a
mapping would need a lookup, so it must be chosen by hand.

Pairs are then assigned greedily from the highest score, each target column
at most once, if the score is at least **0.65**. Your column choices are
assigned first; column pairs from the word list
(`column: machine.serial_no = asset.code`) count as the strongest automatic
evidence.

## 5. Reasons and confidence

Every decision keeps its reason, shown on the Matching screen, in the report
and in the mapping CSV:

| How | Example reason |
| --- | --- |
| Automatic table | "Matched automatically: 76 % similar by table name and column names." |
| Automatic column | "The names are 67 % similar, the descriptions share words, the types fit." (with a confidence) |
| Word list | "Your word list says these column names mean the same." |
| Your choice | "Chosen by you." / "You chose not to copy this column." |

**Confidence** is the column score capped at 100 %, or the table score.

## 6. Uncertain matches

An automatic match is marked **uncertain** when:

- a table's score is below **70 %**, or
- a column's score is below **85 %**.

Uncertain matches are listed under the *Uncertain* filter and in the
report's unresolved items. **Accept** turns one into your choice (it is then
no longer uncertain); **✕** removes it; choosing another target changes it.

## 7. Your choices

Choices made by hand always win over the word list and the score. They are
kept until you reset them, load or clear a file, or swap source and target,
and they are stored in the saved comparison file. Changing a table's target
drops the column choices for that table, because they were made for the
previous target.

## Origin

The algorithm and its thresholds are the original prototype's, recorded in
the specification (§7). Parity tests check that matches and column mappings
equal the prototype's on the sample pair with the word list, with manual
choices, and without a word list ([decision 17](decisions.md)).
