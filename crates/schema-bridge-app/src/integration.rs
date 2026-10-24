//! The "Integration issues" stage: what does not fit when the source goes
//! into the target, in plain language, with the same search and filters as
//! the Problems stage. The findings come from the core library
//! (`integration`) and follow the matching; this file places them.

use dioxus::prelude::*;
use schema_bridge_core::count_by_severity;
use schema_bridge_core::workspace::{FindingFilter, Workspace, rule_counts};

use crate::icons::Icon;
use crate::problems::{FindingCards, FindingFilters, PAGE, card, more_label};
use crate::state::derived;

#[component]
pub fn IntegrationStage(
    workspace: Signal<Workspace>,
    filter: Signal<FindingFilter>,
    go_to_load: EventHandler<()>,
    go_to_matching: EventHandler<()>,
    go_to_plan: EventHandler<()>,
) -> Element {
    let d = derived();
    let mut shown = use_signal(|| PAGE);
    let ws = workspace.read();
    let (s, t) = (ws.source.name.clone(), ws.target.name.clone());
    drop(ws);
    let analysis = d.analysis.read();

    let head = rsx! {
        section { class: "page-head",
            div {
                h2 { "Integration issues" }
                p { class: "lead",
                    "What does not fit when {s} goes into {t}, in plain language. The list follows the matching: change a match and it updates."
                }
            }
            div { class: "page-actions",
                button { class: "btn ghost", onclick: move |_| go_to_matching.call(()), "Review matching" }
                button { class: "btn secondary", onclick: move |_| go_to_plan.call(()),
                    "Open the plan"
                    Icon { name: "arrow", size: 15 }
                }
            }
        }
    };

    let Some(analysis) = analysis.as_ref() else {
        return rsx! {
            {head}
            section { class: "card empty",
                p { class: "empty-title", "Integration issues need both databases" }
                p { "Load the source and the target export first, or try the sample schemas." }
                div {
                    button { class: "btn primary", onclick: move |_| go_to_load.call(()), "Go to Load schemas" }
                }
            }
        };
    };
    let all = &analysis.integration;
    if all.is_empty() {
        return rsx! {
            {head}
            section { class: "card empty success",
                p { class: "empty-title", "Nothing found" }
                p { "Every matched table fits into its target table as far as the structure shows." }
            }
        };
    }

    let current = filter.read().clone();
    let counts = count_by_severity(all);
    let rules = rule_counts(all);
    let matching: Vec<_> = all
        .iter()
        .enumerate()
        .filter(|(_, f)| current.matches_finding(f))
        .map(|(i, f)| card(i, f, "Fitting them together".into(), "finding-db both"))
        .collect();
    let total = matching.len();
    let limit = shown();
    let cards: Vec<_> = matching.into_iter().take(limit).collect();
    let more = total.saturating_sub(limit);
    let status = if total == all.len() {
        format!("Showing all {} issues", all.len())
    } else {
        format!("Showing {} of {} issues", total, all.len())
    };

    rsx! {
        {head}
        FindingFilters {
            filter,
            counts,
            rules,
            databases: Vec::new(),
            placeholder: "Search issues, tables, columns…",
        }
        p { class: "muted shown", role: "status", {status} }
        if cards.is_empty() {
            section { class: "card empty",
                p { class: "empty-title", "No issues match these filters" }
                div {
                    button { class: "btn secondary", onclick: move |_| filter.set(FindingFilter::default()), "Clear filters" }
                }
            }
        } else {
            FindingCards { cards }
            if more > 0 {
                button { class: "btn secondary more", onclick: move |_| shown.set(limit + PAGE), {more_label(more, PAGE)} }
            }
        }
    }
}
