//! Small CSV reader, ported from the prototype's `parseCSV` and `cell`.
//!
//! It is kept instead of a CSV crate because export tools differ in a way the
//! prototype relies on: an unquoted `NULL` or empty field means "no value",
//! while a quoted `"NULL"` or `""` is real text. It also accepts `;` as the
//! separator, which some spreadsheet tools write.

use std::collections::HashMap;

/// One data row: lower-case header name to value. `None` means no value.
pub(crate) type Record = HashMap<String, Option<String>>;

pub(crate) struct Csv {
    pub header: Vec<String>,
    pub rows: Vec<Record>,
}

pub(crate) fn parse(text: &str) -> Csv {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let first_line = text.split('\n').next().unwrap_or("");
    let sep = if first_line.split(';').count() > first_line.split(',').count() {
        ';'
    } else {
        ','
    };

    let mut rows: Vec<Vec<Option<String>>> = Vec::new();
    let mut row: Vec<Option<String>> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut quoted = false;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' {
            in_quotes = true;
            quoted = true;
        } else if c == sep {
            row.push(finish(&mut field, quoted));
            quoted = false;
        } else if c == '\n' {
            row.push(finish(&mut field, quoted));
            rows.push(std::mem::take(&mut row));
            quoted = false;
        } else if c != '\r' {
            field.push(c);
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(finish(&mut field, quoted));
        rows.push(row);
    }

    let header: Vec<String> = if rows.is_empty() {
        Vec::new()
    } else {
        rows.remove(0)
            .into_iter()
            .map(|h| h.unwrap_or_default().trim().to_lowercase())
            .collect()
    };
    let rows = rows
        .into_iter()
        .filter(|r| r.len() > 1)
        .map(|r| {
            header
                .iter()
                .enumerate()
                .map(|(i, h)| (h.clone(), r.get(i).cloned().flatten()))
                .collect()
        })
        .collect();
    Csv { header, rows }
}

/// Ends a field. Quoted text is kept exactly; unquoted text is trimmed and
/// an empty or `NULL` field becomes `None`.
fn finish(field: &mut String, quoted: bool) -> Option<String> {
    let value = std::mem::take(field);
    if quoted {
        return Some(value);
    }
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("NULL") {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// The value of a column, treating a missing column like an empty value.
pub(crate) fn get<'a>(record: &'a Record, column: &str) -> Option<&'a str> {
    record.get(column).and_then(|v| v.as_deref())
}

/// Like [`get`], but an empty string also counts as "no value".
/// The prototype uses JavaScript `||` fallbacks, which treat `""` this way.
pub(crate) fn get_nonempty<'a>(record: &'a Record, column: &str) -> Option<&'a str> {
    get(record, column).filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_quoted_fields_with_commas_quotes_and_line_breaks() {
        let csv = parse("a,b\n\"x, y\",\"say \"\"hi\"\"\nnext line\"\n");
        assert_eq!(csv.header, ["a", "b"]);
        assert_eq!(get(&csv.rows[0], "a"), Some("x, y"));
        assert_eq!(get(&csv.rows[0], "b"), Some("say \"hi\"\nnext line"));
    }

    #[test]
    fn unquoted_null_and_empty_mean_no_value_but_quoted_text_is_kept() {
        let csv = parse("a,b,c,d\nNULL, ,\"NULL\",\"\"\n");
        let r = &csv.rows[0];
        assert_eq!(get(r, "a"), None);
        assert_eq!(get(r, "b"), None);
        assert_eq!(get(r, "c"), Some("NULL"));
        assert_eq!(get(r, "d"), Some(""));
        assert_eq!(get_nonempty(r, "d"), None);
    }

    #[test]
    fn detects_semicolon_separator_bom_and_windows_line_ends() {
        let csv = parse("\u{feff}Table_Name;Column_Name\r\nasset;code\r\n");
        assert_eq!(csv.header, ["table_name", "column_name"]);
        assert_eq!(get(&csv.rows[0], "column_name"), Some("code"));
    }

    #[test]
    fn short_rows_get_no_value_and_single_field_rows_are_skipped() {
        let csv = parse("a,b,c\n1,2\nlonely\n");
        assert_eq!(csv.rows.len(), 1);
        assert_eq!(get(&csv.rows[0], "c"), None);
    }

    #[test]
    fn last_line_without_line_end_is_read() {
        let csv = parse("a,b\n1,2");
        assert_eq!(get(&csv.rows[0], "b"), Some("2"));
    }
}
