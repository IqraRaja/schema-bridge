//! Everything Schema Bridge knows about particular database systems, in one
//! place. This is the extension point for new database adapters.
//!
//! The rest of the library is database-neutral: importers turn an export
//! into the neutral [`Schema`](crate::model::Schema), and checks, matching,
//! integration findings and the load order work on that model alone. Where a
//! system needs its own wording or SQL, the code asks this registry by the
//! system code stored in `Schema::dbms`, and falls back to neutral behaviour
//! for a system it does not know.
//!
//! **Adding a system** means adding one [`DatabaseSystem`] entry to
//! [`SYSTEMS`] (and, when there is one, its export query in
//! `export-queries/`). Type names that the shared vocabulary in `types.rs`
//! does not know yet are added there; names that mean something different
//! in this system go into [`DatabaseSystem::type_overrides`].

use crate::checks::quote;

/// One database system the library knows by name.
#[derive(Debug, Clone, Copy)]
pub struct DatabaseSystem {
    /// Code as written in the `dbms` column of an export, lower case.
    pub code: &'static str,
    pub name: &'static str,
    /// Type names (lower case) that mean something else in this system,
    /// with the neutral name they stand for. Applied at import.
    pub type_overrides: &'static [(&'static str, &'static str)],
    /// The statement that checks an unchecked (`NOT VALID`) link, given the
    /// schema, table and constraint name.
    pub validate_link: Option<fn(&str, &str, &str) -> String>,
    /// How to add column descriptions; `None` when the system has none
    /// (the description coverage check is then skipped).
    pub description_fix: Option<&'static str>,
    /// How a PostgreSQL target reads this system as a source.
    pub foreign_data_wrapper: Option<ForeignDataWrapper>,
    /// The read-only query that exports this system's structure.
    pub export: Option<ExportQuery>,
}

/// A PostgreSQL foreign data wrapper for reading a source system.
#[derive(Debug, Clone, Copy)]
pub struct ForeignDataWrapper {
    pub extension: &'static str,
    /// Server options; `{source}` is replaced by the source's name. Only
    /// placeholders, never real values.
    pub server_options: &'static str,
    pub user_mapping: Option<&'static str>,
}

/// The read-only export query of a system, and how to run it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportQuery {
    /// Where to run the query and how to save the result as CSV.
    pub steps: [&'static str; 3],
    pub query: &'static str,
    /// Whether the query has been run on a real server of this system.
    pub tested: bool,
}

const PASSWORD_PLACEHOLDER: &str = "'<password>'";

pub const SYSTEMS: &[DatabaseSystem] = &[
    DatabaseSystem {
        code: "postgresql",
        name: "PostgreSQL",
        type_overrides: &[],
        validate_link: Some(|schema, table, name| {
            format!(
                "ALTER TABLE {}.{} VALIDATE CONSTRAINT {};",
                quote(schema),
                quote(table),
                quote(name)
            )
        }),
        description_fix: Some(
            "Add COMMENT ON COLUMN for the important columns, starting with the ones you migrate.",
        ),
        foreign_data_wrapper: Some(ForeignDataWrapper {
            extension: "postgres_fdw",
            server_options: "host '<host>', dbname '<{source} database>', port '5432'",
            user_mapping: Some("user '<read-only user>', password '<password>'"),
        }),
        export: Some(ExportQuery {
            steps: [
                "In pgAdmin, select the database and open Tools → Query Tool.",
                "Paste the query and run it with F5.",
                "Click “Save results to file” (download icon) and save as CSV.",
            ],
            query: include_str!("../../../export-queries/postgresql.sql"),
            tested: true,
        }),
    },
    DatabaseSystem {
        code: "mysql",
        name: "MySQL / MariaDB",
        // MySQL's `float` is 4 bytes.
        type_overrides: &[("float", "real")],
        validate_link: None,
        description_fix: Some(
            "Add COMMENT clauses (ALTER TABLE … MODIFY … COMMENT) for the important columns.",
        ),
        foreign_data_wrapper: Some(ForeignDataWrapper {
            extension: "mysql_fdw",
            server_options: "host '<host>', port '3306'",
            user_mapping: Some("username '<read-only user>', password '<password>'"),
        }),
        export: Some(ExportQuery {
            steps: [
                "In MySQL Workbench (or DBeaver, phpMyAdmin), select the database: USE your_database;",
                "Paste the query and run it (⚡ or Ctrl+Enter).",
                "In the result grid, choose “Export recordset to an external file” and pick CSV.",
            ],
            query: include_str!("../../../export-queries/mysql.sql"),
            tested: false,
        }),
    },
    DatabaseSystem {
        code: "sqlserver",
        name: "SQL Server",
        // `timestamp` / `rowversion` is a binary row version, not a time.
        type_overrides: &[("timestamp", "bytea"), ("rowversion", "bytea")],
        validate_link: Some(|schema, table, name| {
            format!("ALTER TABLE [{schema}].[{table}] WITH CHECK CHECK CONSTRAINT [{name}];")
        }),
        description_fix: Some(
            "Add MS_Description properties for the important columns, starting with the ones you migrate.",
        ),
        foreign_data_wrapper: Some(ForeignDataWrapper {
            extension: "tds_fdw",
            server_options: "servername '<host>', port '1433', database '<{source} database>'",
            user_mapping: Some("username '<read-only user>', password '<password>'"),
        }),
        export: Some(ExportQuery {
            steps: [
                "In SSMS, turn on Tools → Options → Query Results → SQL Server → Results to Grid → “Include column headers when copying or saving the results” and “Quote strings containing list separators when saving .csv results”, then open a new query window.",
                "Choose the database, paste the query and press F5.",
                "Right-click the result grid → Save Results As… → CSV. (Azure Data Studio: “Save as CSV” next to the results.)",
            ],
            query: include_str!("../../../export-queries/sqlserver.sql"),
            tested: false,
        }),
    },
    DatabaseSystem {
        code: "oracle",
        name: "Oracle",
        // Oracle's `DATE` includes a time of day.
        type_overrides: &[("date", "timestamp(0) without time zone")],
        validate_link: Some(|schema, table, name| {
            format!("ALTER TABLE {schema}.{table} ENABLE VALIDATE CONSTRAINT {name};")
        }),
        description_fix: Some(
            "Add COMMENT ON COLUMN for the important columns, starting with the ones you migrate.",
        ),
        foreign_data_wrapper: Some(ForeignDataWrapper {
            extension: "oracle_fdw",
            server_options: "dbserver '//<host>:1521/<service>'",
            user_mapping: Some("user '<read-only user>', password '<password>'"),
        }),
        export: Some(ExportQuery {
            steps: [
                "In SQL Developer, connect as the schema owner (or a user who can see it).",
                "Paste the query and run it as a statement (Ctrl+Enter).",
                "Right-click the result grid → Export… → format csv, with header.",
            ],
            query: include_str!("../../../export-queries/oracle.sql"),
            tested: false,
        }),
    },
    DatabaseSystem {
        code: "sqlite",
        name: "SQLite",
        type_overrides: &[],
        validate_link: None,
        // SQLite has no column descriptions.
        description_fix: None,
        foreign_data_wrapper: Some(ForeignDataWrapper {
            extension: "sqlite_fdw",
            server_options: "database '/path/to/file.db'",
            user_mapping: None,
        }),
        export: Some(ExportQuery {
            steps: [
                "Open the file in DB Browser for SQLite → Execute SQL.",
                "Paste the query and run it (▶ or Ctrl+Return).",
                "Click “Save the results view” → Export to CSV. (Command line: .headers on, .mode csv, .once schema.csv, then the query.)",
            ],
            query: include_str!("../../../export-queries/sqlite.sql"),
            tested: true,
        }),
    },
];

/// The system with this code, if the library knows it.
pub fn system(code: &str) -> Option<&'static DatabaseSystem> {
    SYSTEMS.iter().find(|s| s.code == code)
}

/// Display name of a system code; an unknown code is shown as it is.
pub fn system_name(code: &str) -> &str {
    system(code).map_or(code, |s| s.name)
}

/// The neutral type name for a type as a system spells it: lower case, with
/// the system's own overrides applied.
pub fn normalise_type(label: &str, code: &str) -> String {
    let t = label.trim().to_lowercase();
    system(code)
        .and_then(|s| s.type_overrides.iter().find(|(from, _)| *from == t))
        .map_or(t, |(_, to)| to.to_string())
}

/// The statement or instruction that checks an unchecked link.
pub fn validate_link(code: &str, schema: &str, table: &str, name: &str) -> String {
    system(code)
        .and_then(|s| s.validate_link)
        .map_or("enable and validate the constraint.".to_string(), |f| {
            f(schema, table, name)
        })
}

/// How to add column descriptions: `None` when the system has none. An
/// unknown system gets a neutral instruction.
pub fn description_fix(code: &str) -> Option<&'static str> {
    match system(code) {
        Some(s) => s.description_fix,
        None => Some(
            "Add column descriptions (comments) for the important columns, starting with the ones you migrate.",
        ),
    }
}

/// The foreign data wrapper for a source system: extension, server options
/// and user mapping options, with placeholders only.
pub fn foreign_data_wrapper(
    code: &str,
    source_name: &str,
) -> Option<(&'static str, String, Option<String>)> {
    let f = system(code)?.foreign_data_wrapper?;
    debug_assert!(
        f.user_mapping
            .is_none_or(|u| u.contains(PASSWORD_PLACEHOLDER))
    );
    Some((
        f.extension,
        f.server_options.replace("{source}", source_name),
        f.user_mapping.map(str::to_string),
    ))
}

/// The systems that ship a read-only export query, in registry order.
pub fn exportable() -> impl Iterator<Item = (&'static DatabaseSystem, &'static ExportQuery)> {
    SYSTEMS
        .iter()
        .filter_map(|s| s.export.as_ref().map(|e| (s, e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_lower_case_and_names_resolve() {
        for (i, s) in SYSTEMS.iter().enumerate() {
            assert_eq!(s.code, s.code.to_lowercase());
            assert!(SYSTEMS[i + 1..].iter().all(|o| o.code != s.code));
            assert_eq!(system_name(s.code), s.name);
        }
        assert_eq!(system_name("db2"), "db2");
    }

    #[test]
    fn type_overrides_apply_only_in_their_system() {
        assert_eq!(
            normalise_type(" DATE ", "oracle"),
            "timestamp(0) without time zone"
        );
        assert_eq!(normalise_type("DATE", "postgresql"), "date");
        assert_eq!(normalise_type("rowversion", "sqlserver"), "bytea");
        assert_eq!(normalise_type("float", "mysql"), "real");
        assert_eq!(normalise_type("float", "db2"), "float");
    }

    #[test]
    fn an_unknown_system_gets_neutral_behaviour() {
        assert_eq!(
            validate_link("db2", "s", "t", "fk"),
            "enable and validate the constraint."
        );
        assert!(
            description_fix("db2")
                .unwrap()
                .starts_with("Add column descriptions")
        );
        assert!(description_fix("sqlite").is_none());
        assert!(foreign_data_wrapper("db2", "S").is_none());
    }

    #[test]
    fn foreign_data_wrappers_hold_placeholders_only() {
        for s in SYSTEMS {
            let (_, server, user) = foreign_data_wrapper(s.code, "Src").unwrap();
            assert!(!server.contains("{source}"));
            assert!(user.is_none_or(|u| u.contains("'<password>'")));
        }
    }

    /// The query without comments and without the text inside '…'.
    fn code(query: &str) -> String {
        let mut out = String::new();
        for line in query.lines() {
            let mut in_text = false;
            let mut chars = line.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '\'' {
                    in_text = !in_text;
                } else if !in_text && c == '-' && chars.peek() == Some(&'-') {
                    break;
                } else if !in_text {
                    out.push(c);
                }
            }
            out.push('\n');
        }
        out.to_uppercase()
    }

    #[test]
    fn every_export_query_only_reads() {
        assert_eq!(exportable().count(), 5);
        for (s, q) in exportable() {
            let code = code(q.query);
            for word in [
                "INSERT", "UPDATE", "DELETE", "DROP", "ALTER", "CREATE", "TRUNCATE", "GRANT",
                "EXEC",
            ] {
                let found = code
                    .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .any(|w| w == word);
                assert!(!found, "{} contains {word}", s.code);
            }
            assert!(code.contains("SELECT"));
        }
    }
}
