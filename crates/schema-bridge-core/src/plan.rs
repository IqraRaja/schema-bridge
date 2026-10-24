//! The load order of the integration plan: a port of the prototype's
//! `loadOrder`. A target table can only be loaded after the tables it links
//! to, so the tables are sorted along the target's links.

use crate::matching::TableMapping;
use crate::model::{Schema, table_name};

/// One target table in the load order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadStep {
    /// Key of the target table.
    pub table: String,
    /// Names of the source tables loaded into it. Empty for a table that is
    /// not loaded but must already hold the rows other tables link to.
    pub from: Vec<String>,
    /// True when the table is part of a cycle of links, so no order works
    /// without loading in two passes.
    pub cycle: bool,
}

/// The target tables that receive data, plus the tables they need (a link
/// that is required or filled from the source), sorted so that every table
/// comes after the tables it links to. Ties are broken by table key; tables
/// on a cycle come last, marked.
pub fn load_order(target: &Schema, mappings: &[TableMapping]) -> Vec<LoadStep> {
    // Tables that receive data, in the order of the mappings.
    let mut need: Vec<LoadStep> = Vec::new();
    for p in mappings {
        let name = table_name(&p.source).to_string();
        match need.iter_mut().find(|s| s.table == p.target) {
            Some(step) => step.from.push(name),
            None => need.push(LoadStep {
                table: p.target.clone(),
                from: vec![name],
                cycle: false,
            }),
        }
    }
    let mapped = |table: &str, column: &str| {
        mappings.iter().any(|p| {
            p.target == table
                && p.rows
                    .iter()
                    .any(|r| r.target.as_ref().is_some_and(|c| c.name == column))
        })
    };

    // Add the tables they link to, as long as the link must be filled.
    let mut stack: Vec<String> = need.iter().map(|s| s.table.clone()).collect();
    while let Some(key) = stack.pop() {
        let Some(t) = target.table(&key) else {
            continue;
        };
        for f in &t.foreign_keys {
            let known = need.iter().any(|s| s.table == f.references);
            if f.references == key || target.table(&f.references).is_none() || known {
                continue;
            }
            let first = f.columns.first().map(String::as_str).unwrap_or("");
            let required = t.column(first).is_some_and(|c| !c.nullable);
            if required || mapped(&key, first) {
                need.push(LoadStep {
                    table: f.references.clone(),
                    from: Vec::new(),
                    cycle: false,
                });
                stack.push(f.references.clone());
            }
        }
    }

    // Topological sort (Kahn), parents first.
    let keys: Vec<&str> = need.iter().map(|s| s.table.as_str()).collect();
    let index = |k: &str| keys.iter().position(|x| *x == k);
    let mut waiting = vec![0usize; keys.len()];
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); keys.len()];
    for (i, k) in keys.iter().enumerate() {
        let Some(t) = target.table(k) else { continue };
        for f in &t.foreign_keys {
            if f.references == *k {
                continue;
            }
            if let Some(parent) = index(&f.references) {
                if !children[parent].contains(&i) {
                    children[parent].push(i);
                    waiting[i] += 1;
                }
            }
        }
    }
    let mut ready: Vec<usize> = (0..keys.len()).filter(|i| waiting[*i] == 0).collect();
    ready.sort_by_key(|i| keys[*i]);
    let mut ready: std::collections::VecDeque<usize> = ready.into();
    let mut order: Vec<usize> = Vec::new();
    while let Some(i) = ready.pop_front() {
        order.push(i);
        for &c in &children[i] {
            waiting[c] -= 1;
            if waiting[c] == 0 {
                ready.push_back(c);
            }
        }
    }
    let mut steps: Vec<LoadStep> = order.iter().map(|i| need[*i].clone()).collect();
    for (i, step) in need.iter().enumerate() {
        if !order.contains(&i) {
            steps.push(LoadStep {
                cycle: true,
                ..step.clone()
            });
        }
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Column, ForeignKey, Table};

    fn table(name: &str, links: &[(&str, &str, bool)]) -> Table {
        let mut t = Table {
            key: format!("public.{name}"),
            schema: "public".into(),
            name: name.into(),
            columns: vec![Column {
                name: "id".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        for (column, to, nullable) in links {
            t.columns.push(Column {
                name: column.to_string(),
                nullable: *nullable,
                ..Default::default()
            });
            t.foreign_keys.push(ForeignKey {
                columns: vec![column.to_string()],
                references: format!("public.{to}"),
                ..Default::default()
            });
        }
        t
    }

    fn mapping(source: &str, target: &str) -> TableMapping {
        TableMapping {
            source: format!("src.{source}"),
            target: format!("public.{target}"),
            rows: Vec::new(),
            required: Vec::new(),
            trace: None,
        }
    }

    fn keys(steps: &[LoadStep]) -> Vec<String> {
        steps
            .iter()
            .map(|s| format!("{}{}", table_name(&s.table), if s.cycle { "*" } else { "" }))
            .collect()
    }

    #[test]
    fn parents_come_first_and_required_parents_are_added() {
        let target = Schema {
            tables: vec![
                table(
                    "order_line",
                    &[("order_id", "order", false), ("note_id", "note", true)],
                ),
                table("order", &[("customer_id", "customer", false)]),
                table("customer", &[]),
                table("note", &[]),
            ],
            ..Default::default()
        };
        let steps = load_order(
            &target,
            &[mapping("line", "order_line"), mapping("o", "order")],
        );
        // `note` is optional and not filled, so it is not needed.
        assert_eq!(keys(&steps), ["customer", "order", "order_line"]);
        assert!(steps[0].from.is_empty());
        assert_eq!(steps[1].from, ["o"]);
    }

    #[test]
    fn a_cycle_is_marked() {
        let target = Schema {
            tables: vec![
                table("a", &[("b_id", "b", false)]),
                table("b", &[("a_id", "a", false), ("parent_id", "b", true)]),
                table("c", &[]),
            ],
            ..Default::default()
        };
        let steps = load_order(&target, &[mapping("x", "a"), mapping("y", "c")]);
        assert_eq!(keys(&steps), ["c", "a*", "b*"]);
    }
}
