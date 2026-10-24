//! Matching tables and columns of the source to the target: a port of the
//! prototype's `parseDict`, `toks`, `dice`, `colSim`, `tableScore`,
//! `matchTables` and `mapColumns`.
//!
//! Names are split into words and compared with the Dice coefficient. The
//! word list teaches which words, column names and tables mean the same, and
//! the user's own choices (overrides) are stronger than both.

use std::collections::{BTreeMap, BTreeSet};

use crate::checks::{Severity, has_text, is_unique_column};
use crate::model::{Column, Schema, Table, table_name};
use crate::types::{Category, Compatibility, Fit, compatibility, type_info};

/// Minimum score of an automatic table match (specification §7).
pub const TABLE_THRESHOLD: f64 = 0.55;
/// Minimum score of an automatic column match.
pub const COLUMN_THRESHOLD: f64 = 0.65;
/// Below this score no "closest candidate" is shown.
const CANDIDATE_THRESHOLD: f64 = 0.3;

/// Words ignored when descriptions are compared (English and German).
const STOP: &[&str] = &[
    "the", "and", "for", "der", "die", "das", "und", "von", "mit", "ist", "ein", "eine", "einer",
    "zur", "zum", "des", "den", "dem", "auf", "aus", "bei", "this", "that", "with", "from",
    "which", "welche", "oder", "nicht", "wird", "werden", "sind", "kann",
];

/// The user's word list: which words, column names and tables mean the same.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WordList {
    /// One word for another: `component = device`.
    pub words: BTreeMap<String, String>,
    /// One whole column name for another: `limit_min = num_min_value`.
    pub column_names: BTreeMap<String, String>,
    /// Table pairs: `table: run = session`. The value `none` (or `-`) means
    /// the table has no place in the target.
    pub tables: BTreeMap<String, String>,
    /// Column pairs: `column: machine.serial_no = asset.code`.
    pub columns: BTreeMap<String, String>,
    /// Table name patterns that stay in the source; `*` matches anything.
    pub skip: Vec<String>,
    /// Lines that are not a rule, with their line number (from 1).
    pub unrecognised: Vec<(usize, String)>,
}

impl WordList {
    /// Reads a word list. Text after `#` is a note. Lines that are not a
    /// rule are collected in [`WordList::unrecognised`] and otherwise ignored.
    pub fn parse(text: &str) -> WordList {
        let mut list = WordList::default();
        for (i, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = after_label(line, "skip") {
                if !rest.is_empty() {
                    list.skip.extend(
                        rest.split(',')
                            .map(|s| s.trim().to_lowercase())
                            .filter(|s| !s.is_empty()),
                    );
                    continue;
                }
            } else if let Some((a, b)) = after_label(line, "table").and_then(pair) {
                list.tables.insert(a.to_lowercase(), b.to_lowercase());
                continue;
            } else if let Some((a, b)) = after_label(line, "column").and_then(pair) {
                list.columns.insert(a.to_lowercase(), b.to_lowercase());
                continue;
            }
            if let Some((a, b)) = pair(line) {
                let (a, b) = (a.to_lowercase(), b.to_lowercase());
                if a.contains('_') || b.contains('_') {
                    list.column_names.insert(a, b);
                } else {
                    list.words.insert(a, b);
                }
                continue;
            }
            list.unrecognised.push((i + 1, raw.trim().to_string()));
        }
        list
    }

    /// Number of rules read.
    pub fn rule_count(&self) -> usize {
        self.words.len()
            + self.column_names.len()
            + self.tables.len()
            + self.columns.len()
            + self.skip.len()
    }

    /// Whether a (lower-case) table name matches a skip pattern.
    pub fn skips(&self, table: &str) -> bool {
        self.skip.iter().any(|p| glob(p, table))
    }
}

/// `skip: …` → `…` for a label compared without case, else `None`.
fn after_label<'a>(line: &'a str, label: &str) -> Option<&'a str> {
    let head = line.get(..label.len())?;
    if !head.eq_ignore_ascii_case(label) {
        return None;
    }
    line[label.len()..]
        .trim_start()
        .strip_prefix(':')
        .map(str::trim)
}

/// `a = b` with one word on each side (the prototype's `^(\S+)\s*=\s*(\S+)$`,
/// where the left side takes as much as it can).
fn pair(s: &str) -> Option<(&str, &str)> {
    let plain = |w: &str| !w.is_empty() && !w.contains(char::is_whitespace);
    s.char_indices()
        .filter(|(_, c)| *c == '=')
        .rev()
        .map(|(i, _)| (s[..i].trim_end(), s[i + 1..].trim_start()))
        .find(|(a, b)| plain(a) && plain(b))
}

/// Matches a name against a pattern in which `*` stands for anything.
fn glob(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == name;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !name.starts_with(first) || !name.ends_with(last) || name.len() < first.len() + last.len() {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for middle in &parts[1..parts.len() - 1] {
        match rest.find(middle) {
            Some(at) => rest = &rest[at + middle.len()..],
            None => return false,
        }
    }
    true
}

/// Splits a name into lower-case words (snake_case and camelCase), drops a
/// plural `s`, treats `id`, `uuid` and `pk` as the same word and maps words
/// through the word list.
pub fn tokens(name: &str, words: Option<&WordList>) -> Vec<String> {
    // camelCase → camel_case (ASCII only, as in the prototype).
    let mut spaced = String::with_capacity(name.len() + 4);
    let mut previous_lower = false;
    for c in name.chars() {
        if previous_lower && c.is_ascii_uppercase() {
            spaced.push('_');
        }
        previous_lower = c.is_ascii_lowercase();
        spaced.push(c);
    }
    let is_word_char = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || "äöüß".contains(c);
    spaced
        .to_lowercase()
        .split(|c: char| !is_word_char(c))
        .filter(|w| !w.is_empty())
        .map(|w| {
            if matches!(w, "uuid" | "id" | "pk") {
                return "id".to_string();
            }
            let w = if w.chars().count() > 4 && w.ends_with('s') && !w.ends_with("ss") {
                &w[..w.len() - 1]
            } else {
                w
            };
            words
                .and_then(|d| d.words.get(w))
                .cloned()
                .unwrap_or_else(|| w.to_string())
        })
        .collect()
}

/// The meaningful words of a description.
fn description_tokens(text: Option<&str>) -> Vec<String> {
    tokens(text.unwrap_or(""), None)
        .into_iter()
        .filter(|w| w.chars().count() > 3 && !STOP.contains(&w.as_str()))
        .collect()
}

/// Dice coefficient of two word lists, from 0 (nothing shared) to 1.
pub fn dice(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let a: BTreeSet<&String> = a.iter().collect();
    let b: BTreeSet<&String> = b.iter().collect();
    let shared = a.intersection(&b).count();
    2.0 * shared as f64 / (a.len() + b.len()) as f64
}

/// How similar two column names are, from 0 to 1.
fn column_similarity(a: &Column, b: &Column, words: &WordList) -> f64 {
    let (x, y) = (a.name.to_lowercase(), b.name.to_lowercase());
    if x == y || words.column_names.get(&x) == Some(&y) {
        return 1.0;
    }
    dice(&tokens(&x, Some(words)), &tokens(&y, Some(words)))
}

/// A name split into words once, for comparing it with many others: lower
/// case, and its distinct words sorted (a set, as in the prototype's `dice`).
struct PreparedName {
    lower: String,
    words: Vec<String>,
}

impl PreparedName {
    /// Column names are lower-cased before they are split (as in the
    /// prototype's `colSim`); table names are split as written, so their
    /// camelCase counts.
    fn new(name: &str, split_as_written: bool, words: &WordList) -> Self {
        let lower = name.to_lowercase();
        let mut w = tokens(if split_as_written { name } else { &lower }, Some(words));
        w.sort();
        w.dedup();
        PreparedName { lower, words: w }
    }
}

/// [`dice`] on two sorted word sets, without allocating.
fn dice_sorted(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let (mut i, mut j, mut shared) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                shared += 1;
                i += 1;
                j += 1;
            }
        }
    }
    2.0 * shared as f64 / (a.len() + b.len()) as f64
}

/// [`column_similarity`] on prepared names.
fn prepared_similarity(a: &PreparedName, b: &PreparedName, words: &WordList) -> f64 {
    if a.lower == b.lower || words.column_names.get(&a.lower) == Some(&b.lower) {
        return 1.0;
    }
    dice_sorted(&a.words, &b.words)
}

/// A table with its name and column names split once.
struct PreparedTable {
    name: PreparedName,
    columns: Vec<PreparedName>,
    /// Indexes of the columns that are not part of the primary key.
    non_key: Vec<usize>,
}

impl PreparedTable {
    fn new(t: &Table, words: &WordList) -> Self {
        PreparedTable {
            name: PreparedName::new(&t.name, true, words),
            columns: t
                .columns
                .iter()
                .map(|c| PreparedName::new(&c.name, false, words))
                .collect(),
            non_key: t
                .columns
                .iter()
                .enumerate()
                .filter(|(_, c)| !t.primary_key.contains(&c.name))
                .map(|(i, _)| i)
                .collect(),
        }
    }
}

fn prepared_table_score(s: &PreparedTable, t: &PreparedTable, words: &WordList) -> f64 {
    if s.name.lower == t.name.lower {
        return 1.0;
    }
    let names = dice_sorted(&s.name.words, &t.name.words);
    let hits = s
        .non_key
        .iter()
        .filter(|&&i| {
            t.columns
                .iter()
                .any(|x| prepared_similarity(&s.columns[i], x, words) >= 0.8)
        })
        .count();
    let share = if s.non_key.is_empty() {
        0.0
    } else {
        hits as f64 / s.non_key.len() as f64
    };
    0.6 * names + 0.4 * share
}

/// How well a source table fits a target table: name similarity (weight
/// 0.6) and the share of non-key source columns with a close name in the
/// target (weight 0.4). The same name scores 1.
pub fn table_score(s: &Table, t: &Table, words: &WordList) -> f64 {
    prepared_table_score(
        &PreparedTable::new(s, words),
        &PreparedTable::new(t, words),
        words,
    )
}

/// The user's choice for a source table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableChoice {
    /// Goes into this target table (its key).
    Target(String),
    /// Has no place in the target yet.
    NoTarget,
    /// Stays in the source on purpose.
    Stays,
}

/// Choices the user made by hand. They override the word list and the
/// automatic matching.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Overrides {
    /// By source table key.
    pub tables: BTreeMap<String, TableChoice>,
    /// By (source table key, source column): the target column, or `None`
    /// for "not copied".
    pub columns: BTreeMap<(String, String), Option<String>>,
}

impl Overrides {
    pub fn is_empty(&self) -> bool {
        self.tables.is_empty() && self.columns.is_empty()
    }

    fn column(&self, table: &str, column: &str) -> Option<&Option<String>> {
        self.columns.get(&(table.to_string(), column.to_string()))
    }
}

/// What happens to a source table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    /// Matched by its score.
    Automatic,
    /// Matched by a table pair in the word list.
    WordList,
    /// Matched by the user's choice.
    Manual,
    /// No place in the target yet.
    NoTarget,
    /// Stays in the source on purpose.
    Stays,
}

/// Who decided a table's fate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecidedBy {
    Tool,
    WordList,
    User,
}

impl DecidedBy {
    pub fn label(self) -> &'static str {
        match self {
            DecidedBy::Tool => "automatic",
            DecidedBy::WordList => "word list",
            DecidedBy::User => "you",
        }
    }
}

/// The best-scoring target table, shown even when it was not chosen.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub target: String,
    pub score: f64,
}

/// What happens to one source table.
#[derive(Debug, Clone, PartialEq)]
pub struct TableMatch {
    /// Key of the source table.
    pub source: String,
    /// Key of the target table, if it goes into one.
    pub target: Option<String>,
    pub kind: MatchKind,
    pub decided_by: DecidedBy,
    /// The score of an automatic match.
    pub score: Option<f64>,
    pub candidate: Option<Candidate>,
}

impl TableMatch {
    /// An automatic match whose score is low enough to deserve a look.
    pub fn is_uncertain(&self) -> bool {
        self.kind == MatchKind::Automatic && self.score.is_some_and(|s| s < UNCERTAIN_TABLE_BELOW)
    }

    /// Why this table goes where it goes, in plain words.
    pub fn reason(&self) -> String {
        let percent = |x: f64| format!("{:.0}%", x * 100.0);
        match (self.kind, &self.candidate) {
            (MatchKind::Automatic, _) => format!(
                "Matched automatically: {} similar by table name and column names.",
                percent(self.score.unwrap_or(0.0))
            ),
            (MatchKind::WordList, _) => "Matched by a table pair in your word list.".into(),
            (MatchKind::Manual, _) => "Matched by your choice.".into(),
            (MatchKind::Stays, _) if self.decided_by == DecidedBy::User => {
                "You chose to keep this table in the source.".into()
            }
            (MatchKind::Stays, _) => "A skip rule in your word list keeps it in the source.".into(),
            (MatchKind::NoTarget, _) if self.decided_by == DecidedBy::User => {
                "You marked it as having no place in the target yet.".into()
            }
            (MatchKind::NoTarget, Some(c)) => format!(
                "No target table is similar enough. Closest: {} ({}).",
                table_name(&c.target),
                percent(c.score)
            ),
            (MatchKind::NoTarget, None) => "No target table is similar enough.".into(),
        }
    }
}

/// Matches every source table, in the source's table order.
pub fn match_tables(
    source: &Schema,
    target: &Schema,
    words: &WordList,
    overrides: &Overrides,
) -> Vec<TableMatch> {
    let mut out = Vec::new();
    // Every name is split into words once; scoring compares the prepared sets.
    let prepared_targets: Vec<PreparedTable> = target
        .tables
        .iter()
        .map(|t| PreparedTable::new(t, words))
        .collect();
    for s in &source.tables {
        let n = s.name.to_lowercase();
        let ps = PreparedTable::new(s, words);
        let mut best: Option<&Table> = None;
        let mut best_score = 0.0;
        for (t, pt) in target.tables.iter().zip(&prepared_targets) {
            let v = prepared_table_score(&ps, pt, words);
            if v > best_score {
                best_score = v;
                best = Some(t);
            }
        }
        let candidate = best
            .filter(|_| best_score >= CANDIDATE_THRESHOLD)
            .map(|t| Candidate {
                target: t.key.clone(),
                score: best_score,
            });
        let result = |target: Option<String>, kind, decided_by| TableMatch {
            source: s.key.clone(),
            target,
            kind,
            decided_by,
            score: None,
            candidate: candidate.clone(),
        };

        let chosen = match overrides.tables.get(&s.key) {
            Some(TableChoice::Stays) => Some(result(None, MatchKind::Stays, DecidedBy::User)),
            Some(TableChoice::NoTarget) => Some(result(None, MatchKind::NoTarget, DecidedBy::User)),
            Some(TableChoice::Target(k)) if target.table(k).is_some() => {
                Some(result(Some(k.clone()), MatchKind::Manual, DecidedBy::User))
            }
            // A choice of a table that no longer exists is ignored.
            _ => None,
        };
        if let Some(m) = chosen {
            out.push(m);
            continue;
        }
        if let Some(v) = words.tables.get(&n) {
            if v == "none" || v == "-" {
                out.push(result(None, MatchKind::NoTarget, DecidedBy::WordList));
                continue;
            }
            if let Some(t) = target.tables.iter().find(|t| t.name.to_lowercase() == *v) {
                out.push(result(
                    Some(t.key.clone()),
                    MatchKind::WordList,
                    DecidedBy::WordList,
                ));
                continue;
            }
        }
        if words.skips(&n) {
            out.push(result(None, MatchKind::Stays, DecidedBy::WordList));
            continue;
        }
        match best {
            Some(t) if best_score >= TABLE_THRESHOLD => out.push(TableMatch {
                score: Some(best_score),
                ..result(Some(t.key.clone()), MatchKind::Automatic, DecidedBy::Tool)
            }),
            _ => out.push(result(None, MatchKind::NoTarget, DecidedBy::Tool)),
        }
    }
    out
}

/// How a source column got its target column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnHow {
    Automatic,
    WordList,
    User,
}

impl ColumnHow {
    pub fn label(self) -> &'static str {
        match self {
            ColumnHow::Automatic => "automatic",
            ColumnHow::WordList => "word list",
            ColumnHow::User => "you",
        }
    }
}

/// The result for one source column, from best to worst. `NotCopied` and
/// `NeverCopy` are columns without a target column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ColumnStatus {
    /// Copied as it is.
    Copy,
    /// Copied with a small conversion or a decision.
    Convert,
    /// Copied, but check the data first.
    Check,
    /// No place in the target yet.
    NotCopied,
    /// A password or secret: never copied.
    NeverCopy,
}

impl ColumnStatus {
    pub const ALL: [ColumnStatus; 5] = [
        ColumnStatus::Copy,
        ColumnStatus::Convert,
        ColumnStatus::Check,
        ColumnStatus::NotCopied,
        ColumnStatus::NeverCopy,
    ];

    /// Short code, as in the prototype: `ok`, `convert`, `check`, `none`, `skip`.
    pub fn code(self) -> &'static str {
        match self {
            ColumnStatus::Copy => "ok",
            ColumnStatus::Convert => "convert",
            ColumnStatus::Check => "check",
            ColumnStatus::NotCopied => "none",
            ColumnStatus::NeverCopy => "skip",
        }
    }

    /// The label in the column grid.
    pub fn label(self) -> &'static str {
        match self {
            ColumnStatus::Copy => "copy",
            ColumnStatus::Convert => "convert",
            ColumnStatus::Check => "check",
            ColumnStatus::NotCopied => "not copied",
            ColumnStatus::NeverCopy => "never copy",
        }
    }

    /// What it means for the data, as in the prototype's overview.
    pub fn meaning(self) -> &'static str {
        match self {
            ColumnStatus::Copy => "Copy as they are",
            ColumnStatus::Convert => "Copy with a small conversion",
            ColumnStatus::Check => "Copy, but check first",
            ColumnStatus::NotCopied => "No place in the target yet",
            ColumnStatus::NeverCopy => "Stay in the source on purpose",
        }
    }
}

/// What a note on a column pair is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    /// The types differ.
    Type,
    /// The source link lands on a different table than the target link.
    Relationship,
    /// The source links to a table that has no place in the target.
    LinkWithoutPlace,
    /// The target checks a link the source never checked.
    UncheckedLink,
    /// Empty source values into a required target column.
    EmptyValues,
    /// Non-unique source values into a unique target column.
    Unique,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub severity: Severity,
    pub kind: NoteKind,
    pub text: String,
}

/// One source column and where it goes.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnRow {
    pub source: Column,
    pub target: Option<Column>,
    pub how: Option<ColumnHow>,
    pub compatibility: Option<Compatibility>,
    pub notes: Vec<Note>,
    pub status: ColumnStatus,
    /// Why the tool chose this target column, when it chose by score.
    pub evidence: Option<Evidence>,
    /// True when the user decided this column by hand (a target column or
    /// "not copied").
    pub manual: bool,
}

/// Below this score an automatic column match is shown as uncertain.
pub const UNCERTAIN_COLUMN_BELOW: f64 = 0.85;
/// Below this score an automatic table match is shown as uncertain.
pub const UNCERTAIN_TABLE_BELOW: f64 = 0.7;

/// The parts of an automatic column match's score (specification §7):
/// name similarity, description similarity (weight 0.1) and type fit
/// (+0.1 when it fits, −0.25 when it does not).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Evidence {
    pub names: f64,
    pub descriptions: f64,
    pub type_bonus: f64,
    pub score: f64,
}

impl Evidence {
    /// The score as a share from 0 to 1 (a score can exceed 1 with bonuses).
    pub fn confidence(&self) -> f64 {
        self.score.clamp(0.0, 1.0)
    }
}

impl ColumnRow {
    /// An automatic match whose score is low enough to deserve a look.
    pub fn is_uncertain(&self) -> bool {
        !self.manual
            && self.how == Some(ColumnHow::Automatic)
            && self
                .evidence
                .is_some_and(|e| e.score < UNCERTAIN_COLUMN_BELOW)
    }

    /// Why this column goes where it goes, in plain words.
    pub fn reason(&self) -> String {
        match (self.how, &self.target, self.evidence) {
            (Some(ColumnHow::User), Some(_), _) => "Chosen by you.".into(),
            (None, None, _) if self.manual => "You chose not to copy this column.".into(),
            (None, None, _) if self.status == ColumnStatus::NeverCopy => {
                "Looks like a password or secret, so it is never copied automatically.".into()
            }
            (None, None, _) => "No target column matched well enough.".into(),
            (Some(ColumnHow::WordList), _, None) => {
                "Your word list pairs these two columns.".into()
            }
            (Some(ColumnHow::WordList), _, Some(_)) => {
                "Your word list says these column names mean the same.".into()
            }
            (_, _, Some(e)) => {
                let mut parts = vec![if e.names >= 1.0 {
                    "The names match".to_string()
                } else {
                    format!("The names are {:.0}% similar", e.names * 100.0)
                }];
                if e.descriptions > 0.0 {
                    parts.push("the descriptions share words".into());
                }
                if e.type_bonus > 0.0 {
                    parts.push("the types fit".into());
                } else if e.type_bonus < 0.0 {
                    parts.push("but the types do not fit".into());
                }
                format!("{}.", parts.join(", "))
            }
            _ => String::new(),
        }
    }
}

/// A required target column that nothing in the source fills.
#[derive(Debug, Clone, PartialEq)]
pub struct RequiredColumn {
    pub column: Column,
    /// Key of the table it links to, when it is a link.
    pub link_to: Option<String>,
}

/// The column mapping of one source table into its target table.
#[derive(Debug, Clone, PartialEq)]
pub struct TableMapping {
    /// Key of the source table.
    pub source: String,
    /// Key of the target table.
    pub target: String,
    /// One row per source column, in the source's column order.
    pub rows: Vec<ColumnRow>,
    pub required: Vec<RequiredColumn>,
    /// An unused JSON column of the target that can keep the source ID.
    pub trace: Option<String>,
}

impl TableMapping {
    /// The overall state of the table, for the table list: `Check` when
    /// something must be decided or checked, otherwise the worst column.
    pub fn status(&self) -> ColumnStatus {
        if !self.required.is_empty() || self.rows.iter().any(|r| r.status == ColumnStatus::Check) {
            ColumnStatus::Check
        } else if self.rows.iter().any(|r| r.status == ColumnStatus::Convert) {
            ColumnStatus::Convert
        } else {
            ColumnStatus::Copy
        }
    }
}

/// A password, secret or token column (never copied automatically).
fn is_secret_column(name: &str) -> bool {
    let n = name.to_lowercase();
    ["pass", "password", "passwd", "pwd", "secret", "token"]
        .iter()
        .any(|s| {
            n.strip_suffix(s)
                .is_some_and(|before| before.is_empty() || before.ends_with('_'))
        })
}

/// Maps the columns of one matched table pair.
pub fn map_columns(
    s: &Table,
    t: &Table,
    words: &WordList,
    overrides: &Overrides,
    matches: &[TableMatch],
) -> TableMapping {
    struct Pair<'a> {
        sc: &'a Column,
        tc: &'a Column,
        score: f64,
        how: ColumnHow,
        evidence: Option<Evidence>,
    }

    // Column pairs from the word list (`column: s.a = t.b`).
    let s_prefix = format!("{}.", s.name.to_lowercase());
    let t_prefix = format!("{}.", t.name.to_lowercase());
    let second = |k: &str| k.split('.').nth(1).unwrap_or("").to_string();
    let forced: BTreeMap<String, String> = words
        .columns
        .iter()
        .filter(|(a, b)| a.starts_with(&s_prefix) && b.starts_with(&t_prefix))
        .map(|(a, b)| (second(a), second(b)))
        .collect();

    let mut candidates = Vec::new();
    for sc in &s.columns {
        if overrides.column(&s.key, &sc.name).is_some() {
            continue;
        }
        for tc in &t.columns {
            if forced.get(&sc.name.to_lowercase()) == Some(&tc.name.to_lowercase()) {
                candidates.push(Pair {
                    sc,
                    tc,
                    score: 9.0,
                    how: ColumnHow::WordList,
                    evidence: None,
                });
                continue;
            }
            let names = column_similarity(sc, tc, words);
            if names < 0.5 {
                continue;
            }
            // A link in the target is never filled from a plain value automatically.
            if t.link_of(&tc.name).is_some()
                && s.link_of(&sc.name).is_none()
                && !s.primary_key.contains(&sc.name)
            {
                continue;
            }
            let fit = compatibility(&sc.data_type, &tc.data_type).fit;
            let type_bonus = match fit {
                Fit::Same | Fit::Safe => 0.1,
                Fit::Incompatible => -0.25,
                _ => 0.0,
            };
            let descriptions = dice(
                &description_tokens(sc.comment.as_deref()),
                &description_tokens(tc.comment.as_deref()),
            );
            let score = names + 0.1 * descriptions + type_bonus;
            if score >= COLUMN_THRESHOLD {
                let by_list = words.column_names.get(&sc.name.to_lowercase())
                    == Some(&tc.name.to_lowercase());
                candidates.push(Pair {
                    sc,
                    tc,
                    score,
                    evidence: Some(Evidence {
                        names,
                        descriptions,
                        type_bonus,
                        score,
                    }),
                    how: if by_list {
                        ColumnHow::WordList
                    } else {
                        ColumnHow::Automatic
                    },
                });
            }
        }
    }
    candidates.sort_by(|a, b| b.score.total_cmp(&a.score));

    // The user's choices first, then the best candidates; each column once.
    let mut used_s: BTreeSet<&str> = BTreeSet::new();
    let mut used_t: BTreeSet<&str> = BTreeSet::new();
    let mut assigned: BTreeMap<&str, (&Column, ColumnHow, Option<Evidence>)> = BTreeMap::new();
    for sc in &s.columns {
        let Some(choice) = overrides.column(&s.key, &sc.name) else {
            continue;
        };
        used_s.insert(&sc.name);
        if let Some(tc) = choice.as_ref().and_then(|name| t.column(name)) {
            if used_t.insert(&tc.name) {
                assigned.insert(&sc.name, (tc, ColumnHow::User, None));
            }
        }
    }
    for c in &candidates {
        if used_s.contains(c.sc.name.as_str()) || used_t.contains(c.tc.name.as_str()) {
            continue;
        }
        used_s.insert(&c.sc.name);
        used_t.insert(&c.tc.name);
        assigned.insert(&c.sc.name, (c.tc, c.how, c.evidence));
    }

    let target_of = |source_key: &str| {
        matches
            .iter()
            .find(|m| m.source == source_key)
            .and_then(|m| m.target.clone())
    };
    let rows = s
        .columns
        .iter()
        .map(|sc| {
            let manual = overrides.column(&s.key, &sc.name).is_some();
            let Some(&(tc, how, evidence)) = assigned.get(sc.name.as_str()) else {
                return ColumnRow {
                    source: sc.clone(),
                    target: None,
                    how: None,
                    compatibility: None,
                    notes: Vec::new(),
                    evidence: None,
                    manual,
                    status: if is_secret_column(&sc.name) {
                        ColumnStatus::NeverCopy
                    } else {
                        ColumnStatus::NotCopied
                    },
                };
            };
            let cp = compatibility(&sc.data_type, &tc.data_type);
            let mut notes = Vec::new();
            let mut note = |severity, kind, text: String| notes.push(Note { severity, kind, text });
            match cp.fit {
                Fit::Loss | Fit::Incompatible => {
                    note(Severity::High, NoteKind::Type, cp.note.clone())
                }
                Fit::Convert => note(Severity::Medium, NoteKind::Type, cp.note.clone()),
                _ => {}
            }
            match (s.link_of(&sc.name), t.link_of(&tc.name)) {
                (Some(sf), Some(tf)) => match target_of(&sf.references) {
                    Some(expected) if expected != tf.references => note(
                        Severity::High,
                        NoteKind::Relationship,
                        format!(
                            "Source links to {} (which goes into {}), but the target column links to {}. The relationship does not line up.",
                            table_name(&sf.references),
                            table_name(&expected),
                            table_name(&tf.references)
                        ),
                    ),
                    Some(_) => {}
                    None => note(
                        Severity::Medium,
                        NoteKind::LinkWithoutPlace,
                        format!(
                            "Source links to {}, which has no place in the target yet.",
                            table_name(&sf.references)
                        ),
                    ),
                },
                (None, Some(tf)) => note(
                    Severity::Medium,
                    NoteKind::UncheckedLink,
                    format!(
                        "The target checks this link against {}, the source never did. Some values may be rejected.",
                        table_name(&tf.references)
                    ),
                ),
                _ => {}
            }
            if sc.nullable && !tc.nullable && !has_text(&tc.default) {
                note(
                    Severity::Medium,
                    NoteKind::EmptyValues,
                    "Source allows empty values, target requires a value.".into(),
                );
            }
            if is_unique_column(t, &tc.name) && !is_unique_column(s, &sc.name) {
                note(
                    Severity::High,
                    NoteKind::Unique,
                    "Target needs unique values, the source does not guarantee them.".into(),
                );
            }
            let status = if notes.iter().any(|n| n.severity == Severity::High) {
                ColumnStatus::Check
            } else if !notes.is_empty() {
                ColumnStatus::Convert
            } else {
                ColumnStatus::Copy
            };
            ColumnRow {
                source: sc.clone(),
                target: Some(tc.clone()),
                how: Some(how),
                compatibility: Some(cp),
                notes,
                status,
                evidence,
                manual,
            }
        })
        .collect();

    let used: BTreeSet<&str> = assigned.values().map(|(c, _, _)| c.name.as_str()).collect();
    let required = t
        .columns
        .iter()
        .filter(|c| !c.nullable && !has_text(&c.default) && !used.contains(c.name.as_str()))
        .map(|c| RequiredColumn {
            column: c.clone(),
            link_to: t.link_of(&c.name).map(|f| f.references.clone()),
        })
        .collect();
    let trace = t
        .columns
        .iter()
        .find(|c| {
            type_info(&c.data_type).category == Category::Json
                && !used.contains(c.name.as_str())
                && [
                    "add_info",
                    "extra",
                    "meta",
                    "info",
                    "attributes",
                    "properties",
                ]
                .iter()
                .any(|w| c.name.contains(w))
        })
        .map(|c| c.name.clone());

    TableMapping {
        source: s.key.clone(),
        target: t.key.clone(),
        rows,
        required,
        trace,
    }
}

/// Table matches and the column mapping of every matched table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Matching {
    /// One per source table, in the source's table order.
    pub tables: Vec<TableMatch>,
    /// One per source table that goes into a target table, same order.
    pub mappings: Vec<TableMapping>,
}

impl Matching {
    pub fn table(&self, source_key: &str) -> Option<&TableMatch> {
        self.tables.iter().find(|m| m.source == source_key)
    }

    pub fn mapping(&self, source_key: &str) -> Option<&TableMapping> {
        self.mappings.iter().find(|m| m.source == source_key)
    }

    /// How many source columns end up in each [`ColumnStatus`], in
    /// [`ColumnStatus::ALL`] order. Columns of tables without a target count
    /// as "not copied", or "never copy" when the table stays in the source.
    pub fn column_counts(&self, source: &Schema) -> [usize; 5] {
        let mut counts = [0; 5];
        for s in &source.tables {
            if let Some(p) = self.mapping(&s.key) {
                for r in &p.rows {
                    counts[r.status as usize] += 1;
                }
            } else {
                let stays = self
                    .table(&s.key)
                    .is_some_and(|m| m.kind == MatchKind::Stays);
                let status = if stays {
                    ColumnStatus::NeverCopy
                } else {
                    ColumnStatus::NotCopied
                };
                counts[status as usize] += s.columns.len();
            }
        }
        counts
    }
}

/// The review states the Matching stage filters by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Review {
    /// Goes into a target table.
    Matched,
    /// Has no place in the target yet.
    Unmatched,
    /// An automatic match with a low score, for the table or a column.
    Uncertain,
    /// Something was decided by hand, for the table or a column.
    Manual,
    /// Stays in the source on purpose.
    Stays,
}

impl Review {
    pub const ALL: [Review; 5] = [
        Review::Matched,
        Review::Unmatched,
        Review::Uncertain,
        Review::Manual,
        Review::Stays,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Review::Matched => "Matched",
            Review::Unmatched => "No place yet",
            Review::Uncertain => "Uncertain",
            Review::Manual => "Changed by you",
            Review::Stays => "Stays in source",
        }
    }
}

/// Which source tables the Matching stage lists.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableFilter {
    /// Searched, ignoring case, in the source and target table names and in
    /// the column names on both sides.
    pub text: String,
    pub review: Option<Review>,
}

/// How the table list is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TableSort {
    /// Source order (by name).
    #[default]
    Name,
    /// What needs attention first: checks, conversions, no place, ready,
    /// staying; uncertain matches first within each.
    Attention,
    /// Most columns first.
    Size,
}

impl Matching {
    /// Whether a source table is in a review state.
    pub fn is_in(&self, m: &TableMatch, review: Review) -> bool {
        let rows = self
            .mapping(&m.source)
            .map(|p| p.rows.as_slice())
            .unwrap_or(&[]);
        match review {
            Review::Matched => m.target.is_some(),
            Review::Unmatched => m.kind == MatchKind::NoTarget,
            Review::Stays => m.kind == MatchKind::Stays,
            Review::Uncertain => m.is_uncertain() || rows.iter().any(ColumnRow::is_uncertain),
            Review::Manual => m.decided_by == DecidedBy::User || rows.iter().any(|r| r.manual),
        }
    }

    /// How many source tables are in a review state.
    pub fn count(&self, review: Review) -> usize {
        self.tables.iter().filter(|m| self.is_in(m, review)).count()
    }

    /// The source tables the filter lets through, in the chosen order.
    pub fn filtered<'a>(
        &'a self,
        source: &Schema,
        filter: &TableFilter,
        sort: TableSort,
    ) -> Vec<&'a TableMatch> {
        let text = filter.text.trim().to_lowercase();
        let has = |name: &str| name.to_lowercase().contains(&text);
        let mut out: Vec<&TableMatch> = self
            .tables
            .iter()
            .filter(|m| filter.review.is_none_or(|r| self.is_in(m, r)))
            .filter(|m| {
                if text.is_empty() {
                    return true;
                }
                let source_columns = source
                    .table(&m.source)
                    .is_some_and(|t| t.columns.iter().any(|c| has(&c.name)));
                let target_columns = self.mapping(&m.source).is_some_and(|p| {
                    p.rows
                        .iter()
                        .filter_map(|r| r.target.as_ref())
                        .any(|c| has(&c.name))
                });
                has(table_name(&m.source))
                    || m.target.as_deref().is_some_and(|t| has(table_name(t)))
                    || source_columns
                    || target_columns
            })
            .collect();
        let rank = |m: &TableMatch| -> (u8, bool) {
            let status = match (self.mapping(&m.source), m.kind) {
                (Some(p), _) => match p.status() {
                    ColumnStatus::Check => 0,
                    ColumnStatus::Convert => 1,
                    _ => 3,
                },
                (None, MatchKind::Stays) => 4,
                (None, _) => 2,
            };
            (status, !self.is_in(m, Review::Uncertain))
        };
        let columns = |m: &TableMatch| source.table(&m.source).map_or(0, |t| t.columns.len());
        match sort {
            TableSort::Name => {}
            TableSort::Attention => out.sort_by_key(|m| rank(m)),
            TableSort::Size => out.sort_by_key(|m| std::cmp::Reverse(columns(m))),
        }
        out
    }
}

/// The review states of a single column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnReview {
    Mapped,
    NotMapped,
    Uncertain,
    Manual,
    /// Copied, but something must be checked first.
    NeedsCheck,
}

impl ColumnReview {
    pub const ALL: [ColumnReview; 5] = [
        ColumnReview::Mapped,
        ColumnReview::NotMapped,
        ColumnReview::Uncertain,
        ColumnReview::Manual,
        ColumnReview::NeedsCheck,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ColumnReview::Mapped => "Mapped",
            ColumnReview::NotMapped => "Not copied",
            ColumnReview::Uncertain => "Uncertain",
            ColumnReview::Manual => "Changed by you",
            ColumnReview::NeedsCheck => "Needs a check",
        }
    }

    pub fn holds(self, row: &ColumnRow) -> bool {
        match self {
            ColumnReview::Mapped => row.target.is_some(),
            ColumnReview::NotMapped => row.target.is_none(),
            ColumnReview::Uncertain => row.is_uncertain(),
            ColumnReview::Manual => row.manual,
            ColumnReview::NeedsCheck => row.status == ColumnStatus::Check,
        }
    }
}

/// Which columns of a table mapping the column grid shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ColumnFilter {
    /// Searched, ignoring case, in both column names and descriptions.
    pub text: String,
    pub review: Option<ColumnReview>,
}

impl ColumnFilter {
    pub fn matches(&self, row: &ColumnRow) -> bool {
        let text = self.text.trim().to_lowercase();
        let has = |v: &str| v.to_lowercase().contains(&text);
        let text_ok = text.is_empty()
            || has(&row.source.name)
            || row.source.comment.as_deref().is_some_and(has)
            || row
                .target
                .as_ref()
                .is_some_and(|c| has(&c.name) || c.comment.as_deref().is_some_and(has));
        text_ok && self.review.is_none_or(|r| r.holds(row))
    }
}

/// Matches the tables of two schemas and maps the columns of every match.
pub fn match_schemas(
    source: &Schema,
    target: &Schema,
    words: &WordList,
    overrides: &Overrides,
) -> Matching {
    if source.tables.is_empty() || target.tables.is_empty() {
        return Matching::default();
    }
    let tables = match_tables(source, target, words, overrides);
    let mappings = tables
        .iter()
        .filter_map(|m| {
            let s = source.table(&m.source)?;
            let t = target.table(m.target.as_deref()?)?;
            Some(map_columns(s, t, words, overrides, &tables))
        })
        .collect();
    Matching { tables, mappings }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(name: &str, data_type: &str) -> Column {
        Column {
            name: name.into(),
            type_label: data_type.into(),
            data_type: data_type.into(),
            nullable: true,
            ..Default::default()
        }
    }

    fn table(name: &str, columns: Vec<Column>) -> Table {
        Table {
            key: format!("public.{name}"),
            schema: "public".into(),
            name: name.into(),
            columns,
            primary_key: vec!["id".into()],
            ..Default::default()
        }
    }

    fn schema(tables: Vec<Table>) -> Schema {
        Schema {
            dbms: "postgresql".into(),
            tables,
            ..Default::default()
        }
    }

    #[test]
    fn the_word_list_reads_every_kind_of_rule_and_reports_the_rest() {
        let list = WordList::parse(
            "# a note\n\
             Component = Device\n\
             limit_min = num_min_value   # whole column names\n\
             TABLE: run = session\n\
             table: booking = none\n\
             column: machine.serial_no = asset.code\n\
             skip: timetable*, User_* ,\n\
             this line means nothing\n",
        );
        assert_eq!(list.words["component"], "device");
        assert_eq!(list.column_names["limit_min"], "num_min_value");
        assert_eq!(list.tables["run"], "session");
        assert_eq!(list.tables["booking"], "none");
        assert_eq!(list.columns["machine.serial_no"], "asset.code");
        assert_eq!(list.skip, ["timetable*", "user_*"]);
        assert_eq!(list.rule_count(), 7);
        assert_eq!(
            list.unrecognised,
            [(8, "this line means nothing".to_string())]
        );
    }

    #[test]
    fn skip_patterns_use_a_star_for_anything() {
        let list = WordList::parse("skip: user_*, *log*, icon, a*b*c");
        assert!(list.skips("user_group"));
        assert!(list.skips("component_log_2024"));
        assert!(list.skips("icon"));
        assert!(!list.skips("icons"));
        assert!(list.skips("abc") && list.skips("a_b_c") && !list.skips("acb"));
    }

    #[test]
    fn names_are_split_singularised_and_translated() {
        let words = WordList::parse("component = device");
        assert_eq!(
            tokens("componentFunctions", Some(&words)),
            ["device", "function"]
        );
        assert_eq!(tokens("UUID_parent_pk", None), ["id", "parent", "id"]);
        // Short words and double s keep their ending.
        assert_eq!(tokens("bus_address", None), ["bus", "address"]);
    }

    #[test]
    fn dice_compares_word_sets() {
        let w = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(dice(&w(&["a", "b"]), &w(&["b", "c"])), 0.5);
        assert_eq!(dice(&w(&["a"]), &w(&[])), 0.0);
        assert_eq!(dice(&w(&["a", "a"]), &w(&["a"])), 1.0);
    }

    #[test]
    fn overrides_beat_the_word_list_which_beats_the_score() {
        let source = schema(vec![
            table("device", vec![column("id", "integer")]),
            table("log", vec![column("id", "integer")]),
        ]);
        let target = schema(vec![
            table("device", vec![column("id", "integer")]),
            table("journal", vec![column("id", "integer")]),
        ]);
        let words = WordList::parse("table: log = journal\nskip: dev*");
        let m = match_tables(&source, &target, &words, &Overrides::default());
        // The skip rule is weaker than a table pair but stronger than the score.
        assert_eq!(m[0].kind, MatchKind::Stays);
        assert_eq!(m[1].kind, MatchKind::WordList);
        assert_eq!(m[1].target.as_deref(), Some("public.journal"));

        let mut overrides = Overrides::default();
        overrides.tables.insert(
            "public.device".into(),
            TableChoice::Target("public.device".into()),
        );
        overrides.tables.insert(
            "public.log".into(),
            TableChoice::Target("public.gone".into()),
        );
        let m = match_tables(&source, &target, &words, &overrides);
        assert_eq!(m[0].kind, MatchKind::Manual);
        assert_eq!(m[0].decided_by, DecidedBy::User);
        // A choice of a missing table is ignored.
        assert_eq!(m[1].kind, MatchKind::WordList);
    }

    #[test]
    fn a_target_link_is_never_filled_from_a_plain_value() {
        let mut s = table(
            "item",
            vec![column("id", "integer"), column("owner", "text")],
        );
        s.primary_key = vec!["id".into()];
        let mut t = table(
            "item",
            vec![column("id", "integer"), column("owner_id", "integer")],
        );
        t.foreign_keys.push(crate::model::ForeignKey {
            columns: vec!["owner_id".into()],
            references: "public.person".into(),
            ..Default::default()
        });
        let words = WordList::default();
        let p = map_columns(&s, &t, &words, &Overrides::default(), &[]);
        assert_eq!(p.rows[1].target, None);

        // The user may still choose it; it then becomes a lookup.
        let mut overrides = Overrides::default();
        overrides.columns.insert(
            ("public.item".into(), "owner".into()),
            Some("owner_id".into()),
        );
        let p = map_columns(&s, &t, &words, &overrides, &[]);
        assert_eq!(p.rows[1].how, Some(ColumnHow::User));
        assert!(
            p.rows[1]
                .notes
                .iter()
                .any(|n| n.kind == NoteKind::UncheckedLink)
        );
    }

    #[test]
    fn required_columns_secrets_and_the_trace_column() {
        let s = table(
            "user",
            vec![column("id", "integer"), column("password", "text")],
        );
        let mut name = column("name", "text");
        name.nullable = false;
        let mut created = column("created", "timestamp");
        created.nullable = false;
        created.default = Some("now()".into());
        let t = table(
            "person",
            vec![
                column("id", "integer"),
                name,
                created,
                column("extra_info", "jsonb"),
            ],
        );
        let p = map_columns(&s, &t, &WordList::default(), &Overrides::default(), &[]);
        assert_eq!(p.rows[0].status, ColumnStatus::Copy);
        assert_eq!(p.rows[1].status, ColumnStatus::NeverCopy);
        let required: Vec<&str> = p.required.iter().map(|r| r.column.name.as_str()).collect();
        assert_eq!(required, ["name"]);
        assert_eq!(p.trace.as_deref(), Some("extra_info"));
        assert_eq!(p.status(), ColumnStatus::Check);
    }
}
