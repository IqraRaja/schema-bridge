//! Recognising column types from all supported systems, and judging whether a
//! value of one type fits into a column of another.
//!
//! Ported from the prototype's `tinfo` and `compat`. Two deliberate
//! differences are listed in docs/decisions.md (decision 12).

/// Size used for text without a length limit.
pub const UNLIMITED: u64 = u64::MAX;

/// What kind of value a type holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Bool,
    Text,
    Int,
    Float,
    Numeric,
    Timestamp,
    Date,
    Time,
    Uuid,
    Json,
    Binary,
    Xml,
    Interval,
    /// Anything else: enums, geometry, user-defined types.
    Other,
}

/// A recognised type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeInfo {
    /// The type in lower case without `[]`, `unsigned` or `identity`.
    pub raw: String,
    /// The type name without any `(…)` part, for example `varchar`.
    pub base: String,
    pub category: Category,
    /// Bytes for numbers, maximum characters for text ([`UNLIMITED`] when
    /// there is no limit), 0 when size does not apply.
    pub size: u64,
    /// The first number in brackets, for example 100 in `varchar(100)`.
    pub length: Option<u64>,
    /// For timestamps: whether a time zone is stored.
    pub time_zone: Option<bool>,
    /// True for a list type such as `integer[]`.
    pub array: bool,
}

/// How well a source value fits into a target column, from best to worst.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Fit {
    /// Exactly the same type.
    Same,
    /// Fits without losing anything.
    Safe,
    /// Needs an explicit conversion or a lookup.
    Convert,
    /// Values may be cut, rounded or lose their time zone.
    Loss,
    /// Cannot be converted automatically.
    Incompatible,
}

impl Fit {
    pub const ALL: [Fit; 5] = [
        Fit::Same,
        Fit::Safe,
        Fit::Convert,
        Fit::Loss,
        Fit::Incompatible,
    ];

    /// Short name shown to users.
    pub fn label(self) -> &'static str {
        match self {
            Fit::Same => "Same",
            Fit::Safe => "Safe",
            Fit::Convert => "Requires conversion",
            Fit::Loss => "Potential information loss",
            Fit::Incompatible => "Incompatible",
        }
    }

    /// What the level means for the data, in plain words.
    pub fn meaning(self) -> &'static str {
        match self {
            Fit::Same => "Both columns have the same type; values are copied unchanged.",
            Fit::Safe => "Every value fits into the target column without losing anything.",
            Fit::Convert => {
                "Values must be converted or looked up on the way; the draft SQL marks how."
            }
            Fit::Loss => {
                "Some values may be cut, rounded or lose detail; check the data before loading."
            }
            Fit::Incompatible => {
                "Values cannot be converted automatically; someone must decide how to fill the column."
            }
        }
    }

    /// Short code for CSS classes and files: `same`, `safe`, `convert`, `loss`, `incompatible`.
    pub fn code(self) -> &'static str {
        match self {
            Fit::Same => "same",
            Fit::Safe => "safe",
            Fit::Convert => "convert",
            Fit::Loss => "loss",
            Fit::Incompatible => "incompatible",
        }
    }
}

/// The result of comparing two types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compatibility {
    pub fit: Fit,
    /// Plain-language explanation for the user.
    pub note: String,
    /// True when text must be replaced by the ID of another record.
    pub lookup: bool,
}

/// Recognises a type name as written by any of the supported systems.
pub fn type_info(type_name: &str) -> TypeInfo {
    let mut t = type_name.to_lowercase().trim().to_string();
    let array = t.ends_with("[]");
    if array {
        t.truncate(t.len() - 2);
    }
    let t = strip_identity(&strip_modifiers(&t)).trim().to_string();

    let numbers: Vec<&str> = first_brackets(&t)
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .collect();
    let number = |i: usize| {
        numbers
            .get(i)
            .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
            .and_then(|n| n.parse::<u64>().ok())
    };
    let length = number(0);
    let scale = number(1);
    let base = collapse_spaces(&remove_brackets(&t));
    let b = base.as_str();

    let is = |names: &[&str]| names.contains(&b);
    let (category, size, time_zone) =
        if t == "tinyint(1)" || t == "bit" || t == "bit(1)" || is(&["boolean", "bool"]) {
            (Category::Bool, 0, None)
        } else if is(&[
            "character varying",
            "varchar",
            "varchar2",
            "nvarchar",
            "nvarchar2",
            "national character varying",
            "character",
            "char",
            "nchar",
            "bpchar",
            "name",
        ]) {
            (
                Category::Text,
                length.filter(|l| *l > 0).unwrap_or(UNLIMITED),
                None,
            )
        } else if is(&[
            "text",
            "ntext",
            "tinytext",
            "mediumtext",
            "longtext",
            "citext",
            "clob",
            "nclob",
            "long",
            "string",
        ]) {
            (Category::Text, UNLIMITED, None)
        } else if b == "tinyint" {
            (Category::Int, 1, None)
        } else if is(&["smallint", "int2", "year"]) {
            (Category::Int, 2, None)
        } else if is(&["integer", "int", "int4", "serial", "mediumint"]) {
            (Category::Int, 4, None)
        } else if is(&["bigint", "int8", "bigserial"]) {
            (Category::Int, 8, None)
        } else if is(&["real", "float4", "binary_float"]) {
            (Category::Float, 4, None)
        } else if is(&[
            "double precision",
            "double",
            "float8",
            "float",
            "binary_double",
        ]) {
            (Category::Float, 8, None)
        } else if let Some(l) =
            length.filter(|l| b == "number" && scale.unwrap_or(0) == 0 && *l <= 18)
        {
            // Oracle NUMBER(p,0) is a whole number; its size follows the precision.
            let bytes = if l <= 4 {
                2
            } else if l <= 9 {
                4
            } else {
                8
            };
            (Category::Int, bytes, None)
        } else if is(&["numeric", "decimal", "dec", "number", "money", "smallmoney"]) {
            (Category::Numeric, 0, None)
        } else if is(&["timestamptz", "datetimeoffset"])
            || (b.starts_with("timestamp")
                && (b.contains("with time zone") || b.contains("with local time zone"))
                && !b.contains("without"))
        {
            (Category::Timestamp, 0, Some(true))
        } else if b.starts_with("timestamp") || is(&["datetime", "datetime2", "smalldatetime"]) {
            (Category::Timestamp, 0, Some(false))
        } else if b == "date" {
            (Category::Date, 0, None)
        } else if b.starts_with("time") {
            (Category::Time, 0, None)
        } else if is(&["uuid", "uniqueidentifier"]) || t == "raw(16)" {
            (Category::Uuid, 0, None)
        } else if is(&["json", "jsonb"]) {
            (Category::Json, 0, None)
        } else if is(&[
            "bytea",
            "blob",
            "tinyblob",
            "mediumblob",
            "longblob",
            "binary",
            "varbinary",
            "image",
            "raw",
            "long raw",
            "bfile",
        ]) {
            (Category::Binary, 0, None)
        } else if is(&["xml", "xmltype"]) {
            (Category::Xml, 0, None)
        } else if b.starts_with("interval") {
            (Category::Interval, 0, None)
        } else if b.contains("int") {
            // SQLite type affinity rules for free-form type names.
            (Category::Int, 8, None)
        } else if b.contains("char") || b.contains("clob") || b.contains("text") {
            (Category::Text, UNLIMITED, None)
        } else if b.contains("real") || b.contains("floa") || b.contains("doub") {
            (Category::Float, 8, None)
        } else if b.contains("blob") {
            (Category::Binary, 0, None)
        } else {
            (Category::Other, 0, None)
        };

    let raw = if array { format!("{t}[]") } else { t.clone() };
    TypeInfo {
        raw,
        base: base.clone(),
        category,
        size,
        length,
        time_zone,
        array,
    }
}

/// Judges whether values of the `source` type fit into a `target` column.
pub fn compatibility(source: &str, target: &str) -> Compatibility {
    use Category as C;
    let a = type_info(source);
    let b = type_info(target);
    let result = |fit: Fit, note: &str| Compatibility {
        fit,
        note: note.to_string(),
        lookup: false,
    };

    if a.raw == b.raw {
        return result(Fit::Same, "Same type");
    }
    // Checked before "same kind" (unlike the prototype): a list never fits a single value.
    if a.array != b.array {
        return result(
            Fit::Incompatible,
            if a.array {
                "Source holds a list of values, target holds one. Needs splitting or a link table."
            } else {
                "Source holds one value, target expects a list."
            },
        );
    }
    if a.category == b.category
        && !matches!(a.category, C::Other | C::Timestamp | C::Text)
        && a.size == b.size
    {
        return result(Fit::Safe, "Same kind of value");
    }
    match (a.category, b.category) {
        (C::Text, C::Text) => {
            return if b.size >= a.size {
                result(Fit::Safe, "Text fits")
            } else {
                let source_size = if a.size == UNLIMITED {
                    "any length".to_string()
                } else {
                    a.size.to_string()
                };
                result(
                    Fit::Loss,
                    &format!(
                        "Target allows {} characters, source allows {source_size}. Longer values would be rejected.",
                        b.size
                    ),
                )
            };
        }
        (C::Bool, C::Bool) => return result(Fit::Safe, "Yes/no value"),
        (C::Int, C::Int) => {
            return if b.size >= a.size {
                result(Fit::Safe, "Whole number fits")
            } else {
                result(Fit::Loss, "Target number range is smaller.")
            };
        }
        (C::Int, C::Float | C::Numeric) => return result(Fit::Safe, "Whole number into decimal"),
        (C::Float, C::Float) => {
            return if b.size >= a.size {
                result(Fit::Safe, "Decimal fits")
            } else {
                result(Fit::Loss, "Target keeps fewer digits.")
            };
        }
        (C::Float, C::Numeric) => return result(Fit::Safe, "Decimal fits"),
        (C::Float | C::Numeric, C::Int) => {
            return result(Fit::Loss, "Decimals would be rounded off.");
        }
        (C::Timestamp, C::Timestamp) => {
            return if a.time_zone == b.time_zone {
                result(Fit::Safe, "Time fits")
            } else if a.time_zone == Some(true) {
                result(Fit::Loss, "Time zone is dropped.")
            } else {
                result(
                    Fit::Convert,
                    "Source time has no time zone. Say which time zone it was recorded in.",
                )
            };
        }
        (C::Date, C::Timestamp) => return result(Fit::Safe, "Date becomes midnight"),
        (C::Timestamp, C::Date) => return result(Fit::Loss, "Time of day is dropped."),
        (C::Json, C::Json) => return result(Fit::Safe, "JSON fits"),
        _ => {}
    }
    if a.category == C::Binary || b.category == C::Binary {
        return result(
            Fit::Incompatible,
            "Binary data cannot be converted automatically.",
        );
    }
    if b.category == C::Text {
        return result(
            Fit::Convert,
            "Saved as text. Works, but the target can no longer check the value.",
        );
    }
    if a.category == C::Text && b.category == C::Uuid {
        return Compatibility {
            lookup: true,
            ..result(
                Fit::Convert,
                "Source has text, target expects the ID of another record. Each text value must be looked up and replaced by that ID.",
            )
        };
    }
    if a.category == C::Text
        && matches!(
            b.category,
            C::Int | C::Float | C::Numeric | C::Bool | C::Timestamp | C::Date | C::Json
        )
    {
        return result(
            Fit::Convert,
            &format!(
                "Every value must be a valid {}. Test with a query first.",
                b.base
            ),
        );
    }
    if a.category == C::Bool && b.category == C::Int {
        return result(Fit::Convert, "true/false becomes 1/0.");
    }
    if a.category == C::Other || b.category == C::Other {
        let special = if b.category == C::Other {
            &b.base
        } else {
            &a.base
        };
        return result(
            Fit::Convert,
            &format!("Special type {special}: check the allowed values."),
        );
    }
    result(
        Fit::Incompatible,
        &format!("{} and {} do not fit together.", a.base, b.base),
    )
}

/// Removes ` unsigned`, ` signed` and ` zerofill` (MySQL).
fn strip_modifiers(t: &str) -> String {
    let mut out = String::new();
    let mut rest = t;
    'outer: while let Some(c) = rest.chars().next() {
        if c.is_whitespace() {
            let word_start = rest.trim_start();
            for word in ["unsigned", "signed", "zerofill"] {
                if let Some(after) = word_start.strip_prefix(word) {
                    let boundary = after
                        .chars()
                        .next()
                        .is_none_or(|n| !(n.is_alphanumeric() || n == '_'));
                    if boundary {
                        rest = after;
                        continue 'outer;
                    }
                }
            }
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Removes a trailing ` identity` or ` identity(1,1)` (SQL Server).
fn strip_identity(t: &str) -> String {
    for (i, _) in t.match_indices("identity") {
        let before = &t[..i];
        let space_start = before.trim_end().len();
        if space_start == i {
            continue; // needs whitespace before "identity"
        }
        let after = &t[i + "identity".len()..];
        let ok = after.is_empty()
            || (after.starts_with('(') && after.ends_with(')') && !after.contains('\n'));
        if ok {
            return t[..space_start].to_string();
        }
    }
    t.to_string()
}

/// The text inside the first `( … )`.
fn first_brackets(t: &str) -> Option<&str> {
    let open = t.find('(')?;
    let close = t[open + 1..].find(')')?;
    Some(&t[open + 1..open + 1 + close])
}

/// Removes every `( … )` part.
fn remove_brackets(t: &str) -> String {
    let mut out = String::new();
    let mut rest = t;
    while let Some(open) = rest.find('(') {
        match rest[open + 1..].find(')') {
            Some(close) if !rest[open + 1..open + 1 + close].contains('\n') => {
                out.push_str(&rest[..open]);
                rest = &rest[open + 1 + close + 1..];
            }
            _ => break,
        }
    }
    out.push_str(rest);
    out
}

fn collapse_spaces(t: &str) -> String {
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_never_fits_a_single_value() {
        // The prototype judged these "safe" (see decision 12).
        let c = compatibility("integer[]", "integer");
        assert_eq!(c.fit, Fit::Incompatible);
        assert_eq!(compatibility("uuid", "uuid[]").fit, Fit::Incompatible);
    }

    #[test]
    fn text_into_an_id_column_needs_a_lookup() {
        let c = compatibility("varchar(36)", "uuid");
        assert_eq!(c.fit, Fit::Convert);
        assert!(c.lookup);
    }

    #[test]
    fn modifiers_and_identity_are_ignored() {
        assert_eq!(type_info("INT UNSIGNED").raw, "int");
        assert_eq!(type_info("int identity(1,1)").raw, "int");
        assert_eq!(type_info("unsigned big int").category, Category::Int);
    }

    #[test]
    fn fit_levels_are_ordered_from_best_to_worst() {
        assert!(Fit::Same < Fit::Safe && Fit::Loss < Fit::Incompatible);
    }

    #[test]
    fn every_fit_level_has_a_plain_explanation() {
        for fit in Fit::ALL {
            assert!(!fit.label().is_empty() && fit.meaning().ends_with('.'));
        }
        assert_eq!(Fit::Loss.label(), "Potential information loss");
        // timestamp → date drops the time of day, and says so.
        let c = compatibility("timestamp without time zone", "date");
        assert_eq!(c.fit, Fit::Loss);
        assert_eq!(c.note, "Time of day is dropped.");
        let c = compatibility("varchar(100)", "varchar(20)");
        assert_eq!(c.fit, Fit::Loss);
        assert!(c.note.contains("Longer values would be rejected"));
        assert_eq!(compatibility("text", "uuid").fit, Fit::Convert);
        assert!(compatibility("text", "uuid").lookup);
        assert_eq!(compatibility("bytea", "integer").fit, Fit::Incompatible);
        assert_eq!(compatibility("integer", "bigint").fit, Fit::Safe);
    }

    /// The examples in docs/type-compatibility.md, exactly as documented.
    #[test]
    fn the_documented_examples_are_accurate() {
        let cases = [
            ("integer", "integer", Fit::Same, "Same type"),
            ("integer", "bigint", Fit::Safe, "Whole number fits"),
            ("varchar(20)", "text", Fit::Safe, "Text fits"),
            (
                "integer",
                "numeric(10,2)",
                Fit::Safe,
                "Whole number into decimal",
            ),
            ("date", "timestamp", Fit::Safe, "Date becomes midnight"),
            (
                "timestamp",
                "timestamptz",
                Fit::Convert,
                "Source time has no time zone. Say which time zone it was recorded in.",
            ),
            (
                "text",
                "uuid",
                Fit::Convert,
                "Source has text, target expects the ID of another record. Each text value must be looked up and replaced by that ID.",
            ),
            (
                "text",
                "integer",
                Fit::Convert,
                "Every value must be a valid integer. Test with a query first.",
            ),
            (
                "boolean",
                "integer",
                Fit::Convert,
                "true/false becomes 1/0.",
            ),
            (
                "real",
                "text",
                Fit::Convert,
                "Saved as text. Works, but the target can no longer check the value.",
            ),
            (
                "varchar(100)",
                "varchar(20)",
                Fit::Loss,
                "Target allows 20 characters, source allows 100. Longer values would be rejected.",
            ),
            (
                "bigint",
                "smallint",
                Fit::Loss,
                "Target number range is smaller.",
            ),
            (
                "numeric",
                "integer",
                Fit::Loss,
                "Decimals would be rounded off.",
            ),
            ("timestamp", "date", Fit::Loss, "Time of day is dropped."),
            (
                "timestamptz",
                "timestamp",
                Fit::Loss,
                "Time zone is dropped.",
            ),
            (
                "integer",
                "uuid",
                Fit::Incompatible,
                "integer and uuid do not fit together.",
            ),
            (
                "bytea",
                "integer",
                Fit::Incompatible,
                "Binary data cannot be converted automatically.",
            ),
            (
                "integer[]",
                "integer",
                Fit::Incompatible,
                "Source holds a list of values, target holds one. Needs splitting or a link table.",
            ),
        ];
        for (a, b, fit, note) in cases {
            let c = compatibility(a, b);
            assert_eq!((c.fit, c.note.as_str()), (fit, note), "{a} -> {b}");
        }
    }
}
