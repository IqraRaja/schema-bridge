//! The two databases being compared (the "slots"), the files loaded into
//! them and the generic sample schemas. Plain Rust without UI code, so the
//! application's state logic is tested with the rest of the library.

use crate::checks::{Finding, Severity, check_schema};
use crate::import::{ExportFiles, ExportKind, build_schema};
use crate::integration::integration_findings;
use crate::matching::{Matching, Overrides, TableChoice, TableMapping, WordList, match_schemas};
use crate::model::Schema;
use crate::plan::{LoadStep, load_order};
use crate::sql::{draft_sql, table_sql};

/// Generic sample exports (made-up databases from the repository's `demo/`
/// folder). They go through the same import as a user's own files.
const SAMPLE_SOURCE: &str = include_str!("../../../demo/workshop-sqlite.csv");
const SAMPLE_TARGET: &str = include_str!("../../../demo/catalogue-postgresql.csv");
/// The word list that belongs to the sample schemas.
pub const SAMPLE_WORD_LIST: &str = include_str!("../../../demo/sample-word-list.txt");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    /// The database that is brought in.
    Source,
    /// The database that receives it.
    Target,
}

impl Side {
    pub fn id(self) -> &'static str {
        match self {
            Side::Source => "source",
            Side::Target => "target",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Side::Source => "Source",
            Side::Target => "Target",
        }
    }
}

/// Feedback shown in a slot after the last file action.
#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    Loaded { file: String, kind: ExportKind },
    Rejected { file: String, reason: String },
}

/// One database: its display name, the export files loaded for it, and the
/// model built from them.
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub name: String,
    pub files: ExportFiles,
    pub schema: Option<Schema>,
    pub notice: Option<Notice>,
}

impl Slot {
    fn new(name: &str) -> Self {
        Slot {
            name: name.to_string(),
            files: ExportFiles::default(),
            schema: None,
            notice: None,
        }
    }

    /// Adds an export file and rebuilds the model. A file that is not an
    /// export leaves the loaded files unchanged.
    pub fn add_file(&mut self, file_name: &str, text: String) {
        match self.files.add(text) {
            Ok(kind) => {
                self.schema = Some(build_schema(&self.files));
                self.notice = Some(Notice::Loaded {
                    file: file_name.to_string(),
                    kind,
                });
            }
            Err(error) => {
                self.notice = Some(Notice::Rejected {
                    file: file_name.to_string(),
                    reason: error.to_string(),
                });
            }
        }
    }

    /// Records a file the browser could not read.
    pub fn reject_file(&mut self, file_name: &str, reason: &str) {
        self.notice = Some(Notice::Rejected {
            file: file_name.to_string(),
            reason: reason.to_string(),
        });
    }

    pub fn clear(&mut self) {
        self.files = ExportFiles::default();
        self.schema = None;
        self.notice = None;
    }
}

/// Both databases being compared, the word list and the user's choices.
#[derive(Debug, Clone, PartialEq)]
pub struct Workspace {
    pub source: Slot,
    pub target: Slot,
    /// True while both slots hold the sample schemas.
    pub using_samples: bool,
    /// The word list as the user wrote it (see [`WordList::parse`]).
    pub word_list: String,
    /// Table and column choices made by hand.
    pub overrides: Overrides,
}

impl Workspace {
    pub fn new() -> Self {
        Workspace {
            source: Slot::new("Source"),
            target: Slot::new("Target"),
            using_samples: false,
            word_list: String::new(),
            overrides: Overrides::default(),
        }
    }

    pub fn slot(&self, side: Side) -> &Slot {
        match side {
            Side::Source => &self.source,
            Side::Target => &self.target,
        }
    }

    pub fn slot_mut(&mut self, side: Side) -> &mut Slot {
        match side {
            Side::Source => &mut self.source,
            Side::Target => &mut self.target,
        }
    }

    /// Adds a user's file. The workspace no longer counts as sample data,
    /// and choices made for the previous tables are dropped.
    pub fn add_file(&mut self, side: Side, file_name: &str, text: String) {
        self.slot_mut(side).add_file(file_name, text);
        if matches!(self.slot(side).notice, Some(Notice::Loaded { .. })) {
            self.leave_samples();
            self.overrides = Overrides::default();
        }
    }

    pub fn clear(&mut self, side: Side) {
        self.slot_mut(side).clear();
        self.leave_samples();
        self.overrides = Overrides::default();
    }

    /// The sample word list belongs to the sample schemas; it is removed as
    /// soon as the user's own files come in (unless the user edited it).
    fn leave_samples(&mut self) {
        if self.using_samples && self.word_list == SAMPLE_WORD_LIST {
            self.word_list.clear();
        }
        self.using_samples = false;
    }

    /// Swaps source and target. Choices made by hand no longer apply; the
    /// word list is kept, but it is written source = target, so it needs a
    /// check.
    pub fn swap(&mut self) {
        std::mem::swap(&mut self.source, &mut self.target);
        self.overrides = Overrides::default();
    }

    /// The parsed word list.
    pub fn words(&self) -> WordList {
        WordList::parse(&self.word_list)
    }

    /// The table and column matching, once both databases are loaded.
    pub fn matching(&self) -> Option<Matching> {
        let (source, target) = (self.source.schema.as_ref()?, self.target.schema.as_ref()?);
        Some(match_schemas(
            source,
            target,
            &self.words(),
            &self.overrides,
        ))
    }

    /// Matching, integration findings and load order in one go, once both
    /// databases are loaded.
    pub fn analysis(&self) -> Option<Analysis> {
        let (source, target) = (self.source.schema.as_ref()?, self.target.schema.as_ref()?);
        let matching = self.matching()?;
        let integration = integration_findings(
            source,
            target,
            &self.source.name,
            &self.target.name,
            &matching,
        );
        let load_order = load_order(target, &matching.mappings);
        Some(Analysis {
            matching,
            integration,
            load_order,
        })
    }

    /// The whole draft SQL script for an analysis of this workspace.
    pub fn draft_sql(&self, analysis: &Analysis) -> String {
        match (&self.source.schema, &self.target.schema) {
            (Some(s), Some(t)) => draft_sql(
                s,
                t,
                &self.source.name,
                &self.target.name,
                &analysis.matching.mappings,
                &analysis.load_order,
            ),
            _ => String::new(),
        }
    }

    /// The draft SQL for one table mapping.
    pub fn table_sql(&self, mapping: &TableMapping) -> String {
        match (&self.source.schema, &self.target.schema) {
            (Some(s), Some(t)) => table_sql(mapping, s, t, &self.source.name),
            _ => String::new(),
        }
    }

    /// Sets the user's choice for a source table; `None` lets the tool
    /// decide again. Column choices for that table are dropped, because they
    /// were made for the previous target table.
    pub fn set_table_choice(&mut self, source_table: &str, choice: Option<TableChoice>) {
        match choice {
            Some(c) => self.overrides.tables.insert(source_table.to_string(), c),
            None => self.overrides.tables.remove(source_table),
        };
        self.overrides.columns.retain(|(t, _), _| t != source_table);
    }

    /// Sets the user's choice for a source column: `Some(Some(name))` maps it
    /// to that target column, `Some(None)` means "not copied" and `None`
    /// lets the tool decide again.
    pub fn set_column_choice(
        &mut self,
        source_table: &str,
        column: &str,
        choice: Option<Option<String>>,
    ) {
        let key = (source_table.to_string(), column.to_string());
        match choice {
            Some(c) => self.overrides.columns.insert(key, c),
            None => self.overrides.columns.remove(&key),
        };
    }

    /// Forgets every choice made by hand.
    pub fn reset_choices(&mut self) {
        self.overrides = Overrides::default();
    }

    /// The problems found inside each loaded database, most serious first;
    /// within a severity the source's before the target's, otherwise in the
    /// order the checks report them (as in the prototype).
    pub fn findings(&self) -> Vec<(Side, Finding)> {
        let mut all: Vec<(Side, Finding)> = [Side::Source, Side::Target]
            .into_iter()
            .flat_map(|side| {
                self.slot(side)
                    .schema
                    .iter()
                    .flat_map(check_schema)
                    .map(move |f| (side, f))
            })
            .collect();
        all.sort_by_key(|(side, f)| (f.severity, *side));
        all
    }

    /// Replaces both slots with the generic sample schemas.
    pub fn load_samples(&mut self) {
        let mut source = Slot::new("Workshop (sample)");
        source.add_file("workshop-sqlite.csv", SAMPLE_SOURCE.to_string());
        let mut target = Slot::new("Catalogue (sample)");
        target.add_file("catalogue-postgresql.csv", SAMPLE_TARGET.to_string());
        *self = Workspace {
            source,
            target,
            using_samples: true,
            word_list: SAMPLE_WORD_LIST.to_string(),
            overrides: Overrides::default(),
        };
    }
}

impl Default for Workspace {
    fn default() -> Self {
        Workspace::new()
    }
}

/// Everything worked out from the two databases, the word list and the
/// user's choices: what goes where, what does not fit, and in which order the
/// target tables are loaded.
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    pub matching: Matching,
    /// Problems in fitting the two together, most serious first.
    pub integration: Vec<Finding>,
    pub load_order: Vec<LoadStep>,
}

/// Which findings a list shows. `None` and empty text mean "all".
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FindingFilter {
    pub side: Option<Side>,
    pub severity: Option<Severity>,
    /// Rule id, for example `LINK-IMPLICIT`.
    pub rule: Option<&'static str>,
    /// Searched, ignoring case, in the title, object, area, rule,
    /// explanation and fix.
    pub text: String,
}

impl FindingFilter {
    /// For a finding inside one database.
    pub fn matches(&self, side: Side, finding: &Finding) -> bool {
        self.side.is_none_or(|s| s == side) && self.matches_finding(finding)
    }

    /// Everything except the database (for integration findings).
    pub fn matches_finding(&self, f: &Finding) -> bool {
        let text = self.text.trim().to_lowercase();
        let text_ok = text.is_empty()
            || [
                f.title.as_str(),
                &f.object,
                &f.area,
                f.rule,
                &f.explanation,
                &f.fix,
            ]
            .iter()
            .any(|v| v.to_lowercase().contains(&text));
        text_ok
            && self.severity.is_none_or(|s| s == f.severity)
            && self.rule.is_none_or(|r| r == f.rule)
    }

    /// True when anything narrows the list.
    pub fn is_active(&self) -> bool {
        *self != FindingFilter::default()
    }
}

/// The rule ids among some findings with how often each occurs, in the
/// order they first appear.
pub fn rule_counts<'a>(
    findings: impl IntoIterator<Item = &'a Finding>,
) -> Vec<(&'static str, usize)> {
    let mut out: Vec<(&'static str, usize)> = Vec::new();
    for f in findings {
        match out.iter_mut().find(|(r, _)| *r == f.rule) {
            Some((_, n)) => *n += 1,
            None => out.push((f.rule, 1)),
        }
    }
    out
}

/// Display name of a database system code (see `systems`).
pub use crate::systems::system_name;

/// How a loaded file is described to the user.
pub fn kind_label(kind: ExportKind) -> &'static str {
    match kind {
        ExportKind::Combined => "one-file export",
        ExportKind::Columns => "columns part",
        ExportKind::Constraints => "keys and links part",
        ExportKind::Indexes => "indexes part",
        ExportKind::RowCounts => "row counts part",
    }
}

/// Formats a count with thousands separators: 77388 becomes "77,388".
pub fn grouped(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, d) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(d);
    }
    if n < 0 { format!("-{out}") } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_empty_and_loads_samples_through_the_normal_import() {
        let mut ws = Workspace::new();
        assert!(ws.source.schema.is_none() && ws.target.schema.is_none());

        ws.load_samples();
        assert!(ws.using_samples);
        let source = ws.source.schema.as_ref().expect("source loaded");
        let target = ws.target.schema.as_ref().expect("target loaded");
        assert_eq!(source.dbms, "sqlite");
        assert_eq!(target.dbms, "postgresql");
        assert_eq!(source.stats().tables, 5);
        assert_eq!(target.stats().tables, 7);
    }

    #[test]
    fn a_users_file_replaces_the_sample_flag() {
        let mut ws = Workspace::new();
        ws.load_samples();
        ws.add_file(Side::Target, "mine.csv", SAMPLE_TARGET.to_string());
        assert!(!ws.using_samples);
        assert!(matches!(
            ws.target.notice,
            Some(Notice::Loaded {
                kind: ExportKind::Combined,
                ..
            })
        ));
    }

    #[test]
    fn a_file_that_is_not_an_export_is_rejected_and_keeps_what_was_loaded() {
        let mut ws = Workspace::new();
        ws.load_samples();
        ws.add_file(Side::Source, "prices.csv", "item,price\nbread,2\n".into());
        assert!(matches!(ws.source.notice, Some(Notice::Rejected { .. })));
        assert_eq!(ws.source.schema.as_ref().unwrap().stats().tables, 5);
    }

    #[test]
    fn clearing_a_slot_removes_files_and_model() {
        let mut ws = Workspace::new();
        ws.load_samples();
        ws.clear(Side::Source);
        assert!(ws.source.schema.is_none() && ws.source.files.is_empty());
        assert!(ws.target.schema.is_some());
    }

    #[test]
    fn findings_follow_the_loaded_databases() {
        let mut ws = Workspace::new();
        assert!(ws.findings().is_empty());
        ws.load_samples();
        let all = ws.findings();
        let source = all.iter().filter(|(s, _)| *s == Side::Source).count();
        let target = all.iter().filter(|(s, _)| *s == Side::Target).count();
        assert_eq!((source, target), (6, 10));
        // Most serious first; the one blocking finding is the target's.
        assert_eq!(all[0].1.severity, crate::Severity::Blocking);
        assert_eq!(all[0].0, Side::Target);
        assert!(
            all.windows(2)
                .all(|w| (w[0].1.severity, w[0].0) <= (w[1].1.severity, w[1].0))
        );

        ws.clear(Side::Target);
        assert_eq!(ws.findings().len(), 6);
    }

    #[test]
    fn the_filter_selects_by_database_and_severity() {
        let mut ws = Workspace::new();
        ws.load_samples();
        let all = ws.findings();
        let count =
            |filter: FindingFilter| all.iter().filter(|(s, f)| filter.matches(*s, f)).count();
        assert_eq!(count(FindingFilter::default()), all.len());
        let target = FindingFilter {
            side: Some(Side::Target),
            ..Default::default()
        };
        assert_eq!(count(target.clone()), 10);
        let blocking_in_target = FindingFilter {
            severity: Some(Severity::Blocking),
            ..target
        };
        assert_eq!(count(blocking_in_target.clone()), 1);
        let blocking_in_source = FindingFilter {
            side: Some(Side::Source),
            ..blocking_in_target
        };
        assert_eq!(count(blocking_in_source), 0);

        // Rule type and text search narrow further, ignoring case.
        let implicit = FindingFilter {
            rule: Some("LINK-IMPLICIT"),
            ..Default::default()
        };
        assert!(implicit.is_active() && !FindingFilter::default().is_active());
        assert_eq!(count(implicit), 1);
        let text = FindingFilter {
            text: "  PARENT_asset ".into(),
            ..Default::default()
        };
        assert_eq!(
            count(text.clone()),
            all.iter()
                .filter(|(_, f)| f.object.contains("parent_asset"))
                .count()
        );
        assert!(count(text) >= 2);
        let nothing = FindingFilter {
            text: "no such words anywhere".into(),
            ..Default::default()
        };
        assert_eq!(count(nothing), 0);

        let rules = rule_counts(all.iter().map(|(_, f)| f));
        assert_eq!(rules.iter().map(|(_, n)| n).sum::<usize>(), all.len());
        assert!(rules.contains(&("LINK-WRONG-TARGET", 1)));
    }

    #[test]
    fn the_samples_bring_their_word_list_and_lose_it_with_the_users_files() {
        let mut ws = Workspace::new();
        ws.load_samples();
        assert_eq!(ws.word_list, SAMPLE_WORD_LIST);
        let matching = ws.matching().expect("both loaded");
        assert_eq!(matching.tables.len(), 5);
        assert_eq!(matching.mappings.len(), 4);

        ws.add_file(Side::Source, "mine.csv", SAMPLE_SOURCE.to_string());
        assert!(ws.word_list.is_empty());

        // An edited word list is the user's and is kept.
        ws.load_samples();
        ws.word_list.push_str("\nlogin = orcid\n");
        ws.clear(Side::Target);
        assert!(ws.word_list.ends_with("login = orcid\n"));
        assert!(ws.matching().is_none());
    }

    #[test]
    fn choices_are_kept_until_the_tables_change() {
        let mut ws = Workspace::new();
        ws.load_samples();
        ws.set_column_choice("main.machine", "room", Some(Some("location_id".into())));
        ws.set_column_choice("main.run", "id", Some(None));
        ws.set_table_choice(
            "main.booking",
            Some(TableChoice::Target("public.session".into())),
        );
        assert_eq!(ws.overrides.columns.len(), 2);

        // A new table choice drops the column choices of that table only.
        ws.set_table_choice("main.run", None);
        assert_eq!(ws.overrides.columns.len(), 1);
        ws.set_column_choice("main.machine", "room", None);
        assert!(ws.overrides.columns.is_empty());

        // A rejected file changes nothing; an accepted one drops all choices.
        ws.add_file(Side::Source, "prices.csv", "item,price\nbread,2\n".into());
        assert_eq!(ws.overrides.tables.len(), 1);
        ws.add_file(Side::Source, "mine.csv", SAMPLE_SOURCE.to_string());
        assert!(ws.overrides.is_empty());
    }

    #[test]
    fn swapping_exchanges_the_databases_and_drops_choices() {
        let mut ws = Workspace::new();
        ws.load_samples();
        ws.set_table_choice("main.booking", Some(TableChoice::NoTarget));
        ws.swap();
        assert_eq!(ws.source.schema.as_ref().unwrap().dbms, "postgresql");
        assert_eq!(ws.target.schema.as_ref().unwrap().dbms, "sqlite");
        assert!(ws.overrides.is_empty());
        assert_eq!(ws.word_list, SAMPLE_WORD_LIST);
    }

    #[test]
    fn formats_counts_with_thousands_separators() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(77388), "77,388");
        assert_eq!(grouped(-1234567), "-1,234,567");
    }
}
