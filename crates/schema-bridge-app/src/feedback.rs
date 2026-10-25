//! Feedback the whole application shares: the confirmation asked before an
//! action that would discard the user's work, and the short notices shown
//! after a file is saved or text is copied. Any screen can ask or notify;
//! the application shell shows the dialog and the notices and carries out a
//! confirmed action.

use dioxus::prelude::*;
use schema_bridge_core::workspace::{SAMPLE_TS_WORD_LIST, SAMPLE_WORD_LIST, Side, Workspace};

/// An action that discards work, carried out once the user confirms it.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    LoadSamples,
    LoadTimeseriesSamples,
    Clear(Side),
    Swap,
    ResetChoices,
    /// A saved comparison that was read and opened, waiting to replace the
    /// current one.
    Open(Box<Workspace>),
}

impl Action {
    /// Carries the action out.
    pub fn apply(self, ws: &mut Workspace) {
        match self {
            Action::LoadSamples => ws.load_samples(),
            Action::LoadTimeseriesSamples => ws.load_timeseries_samples(),
            Action::Clear(side) => ws.clear(side),
            Action::Swap => ws.swap(),
            Action::ResetChoices => ws.reset_choices(),
            Action::Open(opened) => *ws = *opened,
        }
    }
}

/// The question shown before an action.
#[derive(Debug, Clone, PartialEq)]
pub struct Confirmation {
    pub title: String,
    pub message: String,
    /// Label of the button that carries the action out.
    pub confirm: String,
    pub action: Action,
}

static PENDING: GlobalSignal<Option<Confirmation>> = Signal::global(|| None);

/// Notices on screen, with a number that identifies each one.
static NOTICES: GlobalSignal<(u64, Vec<(u64, String)>)> = Signal::global(|| (0, Vec::new()));

/// How long a notice stays on screen.
const NOTICE_MS: u32 = 4000;

/// Choices made by hand, counted for the confirmation text.
fn choices(ws: &Workspace) -> usize {
    ws.overrides.tables.len() + ws.overrides.columns.len()
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// What the user would lose if both databases were replaced: `None` when
/// nothing of theirs is loaded (empty slots, or the untouched samples).
fn work_at_stake(ws: &Workspace) -> Option<String> {
    let loaded: Vec<&str> = [Side::Source, Side::Target]
        .into_iter()
        .filter(|s| ws.slot(*s).schema.is_some())
        .map(|s| ws.slot(s).name.as_str())
        .collect();
    let n = choices(ws);
    let edited_words = !ws.word_list.is_empty()
        && ws.word_list != SAMPLE_WORD_LIST
        && ws.word_list != SAMPLE_TS_WORD_LIST;
    if loaded.is_empty() || (ws.using_samples && n == 0 && !edited_words) {
        return None;
    }
    let mut parts = vec![format!("the loaded databases ({})", loaded.join(", "))];
    if n > 0 {
        parts.push(format!(
            "your {}",
            plural(n, "choice made by hand", "choices made by hand")
        ));
    }
    if edited_words {
        parts.push("the word list".into());
    }
    let last = parts.pop().unwrap_or_default();
    Some(if parts.is_empty() {
        last
    } else {
        format!("{} and {last}", parts.join(", "))
    })
}

const SAVE_FIRST: &str = "Save the comparison first if you want to keep it.";

/// Runs the action at once when it discards nothing, otherwise asks first.
pub fn confirm_or_apply(mut workspace: Signal<Workspace>, action: Action) {
    let question = {
        let ws = workspace.read();
        question(&ws, &action)
    };
    match question {
        Some((title, message, confirm)) => {
            *PENDING.write() = Some(Confirmation {
                title,
                message,
                confirm,
                action,
            });
        }
        None => action.apply(&mut workspace.write()),
    }
}

/// The question for an action, or `None` when it discards nothing.
fn question(ws: &Workspace, action: &Action) -> Option<(String, String, String)> {
    let n = choices(ws);
    match action {
        Action::LoadSamples | Action::LoadTimeseriesSamples => {
            let lost = work_at_stake(ws)?;
            Some((
                "Replace the current comparison with the sample?".into(),
                format!("This removes {lost}. {SAVE_FIRST}"),
                "Load the sample".into(),
            ))
        }
        Action::Open(_) => {
            let lost = work_at_stake(ws)?;
            Some((
                "Replace the current comparison?".into(),
                format!("Opening this saved comparison removes {lost}. {SAVE_FIRST}"),
                "Open it".into(),
            ))
        }
        Action::Clear(side) => {
            let slot = ws.slot(*side);
            slot.schema.as_ref()?;
            if ws.using_samples && n == 0 {
                return None;
            }
            let also = if n > 0 {
                format!(
                    " Your {} will also be removed.",
                    plural(n, "choice made by hand", "choices made by hand")
                )
            } else {
                String::new()
            };
            Some((
                format!("Clear the {} database?", side.label().to_lowercase()),
                format!("This removes \"{}\" from the comparison.{also}", slot.name),
                "Clear".into(),
            ))
        }
        Action::Swap => (n > 0).then(|| {
            (
                "Swap source and target?".into(),
                format!(
                    "Your {} no longer {} after the swap and will be removed. The word list is kept.",
                    plural(n, "choice made by hand", "choices made by hand"),
                    if n == 1 { "applies" } else { "apply" }
                ),
                "Swap".into(),
            )
        }),
        Action::ResetChoices => (n > 0).then(|| {
            (
                "Reset all your choices?".into(),
                format!(
                    "This removes your {}; the tool decides every match again.",
                    plural(n, "choice made by hand", "choices made by hand")
                ),
                "Reset".into(),
            )
        }),
    }
}

/// Shows a short notice, for example after a file was saved.
pub fn notify(text: impl Into<String>) {
    let mut notices = NOTICES.write();
    notices.0 += 1;
    let id = notices.0;
    notices.1.push((id, text.into()));
}

fn dismiss(id: u64) {
    NOTICES.write().1.retain(|(i, _)| *i != id);
}

/// The confirmation dialog, shown while a question is pending.
#[component]
pub fn ConfirmDialog(workspace: Signal<Workspace>) -> Element {
    let mut workspace = workspace;
    let Some(c) = PENDING() else {
        return rsx! {};
    };
    let cancel = move || *PENDING.write() = None;
    rsx! {
        div {
            class: "dialog-backdrop",
            onclick: move |_| cancel(),
            div {
                class: "dialog",
                role: "alertdialog",
                aria_modal: "true",
                aria_labelledby: "dialog-title",
                aria_describedby: "dialog-text",
                onclick: move |e| e.stop_propagation(),
                onkeydown: move |e| {
                    if e.key() == Key::Escape {
                        cancel();
                    }
                },
                h2 { id: "dialog-title", {c.title} }
                p { id: "dialog-text", {c.message} }
                div { class: "dialog-actions",
                    button {
                        class: "btn secondary",
                        onmounted: move |e| async move {
                            let _ = e.set_focus(true).await;
                        },
                        onclick: move |_| cancel(),
                        "Cancel"
                    }
                    button {
                        class: "btn danger",
                        onclick: move |_| {
                            if let Some(c) = PENDING.write().take() {
                                c.action.apply(&mut workspace.write());
                            }
                        },
                        {c.confirm}
                    }
                }
            }
        }
    }
}

/// The notices, bottom right; each disappears after a few seconds.
#[component]
pub fn Notices() -> Element {
    let shown = NOTICES.read().1.clone();
    rsx! {
        div { class: "notices", role: "status", aria_live: "polite",
            for (id , text) in shown {
                NoticeItem { key: "{id}", id, text }
            }
        }
    }
}

#[component]
fn NoticeItem(id: u64, text: String) -> Element {
    use_future(move || async move {
        crate::browser::wait_ms(NOTICE_MS).await;
        dismiss(id);
    });
    rsx! {
        div { class: "toast",
            span { class: "toast-mark", aria_hidden: "true", "✓" }
            span { {text} }
            button {
                class: "toast-close",
                aria_label: "Dismiss",
                onclick: move |_| dismiss(id),
                "×"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schema_bridge_core::TableChoice;

    fn samples() -> Workspace {
        let mut ws = Workspace::new();
        ws.load_samples();
        ws
    }

    #[test]
    fn nothing_is_asked_when_nothing_would_be_lost() {
        let ws = Workspace::new();
        assert!(question(&ws, &Action::LoadSamples).is_none());
        let ws = samples();
        // The untouched samples can be loaded again at any time.
        for a in [
            Action::LoadSamples,
            Action::LoadTimeseriesSamples,
            Action::Clear(Side::Source),
            Action::Swap,
            Action::ResetChoices,
            Action::Open(Box::new(Workspace::new())),
        ] {
            assert!(question(&ws, &a).is_none(), "{a:?}");
        }
    }

    #[test]
    fn choices_made_by_hand_are_protected() {
        let mut ws = samples();
        ws.set_table_choice(
            "main.booking",
            Some(TableChoice::Target("public.session".into())),
        );
        for a in [
            Action::Swap,
            Action::ResetChoices,
            Action::Clear(Side::Target),
            Action::LoadSamples,
        ] {
            let (_, message, _) = question(&ws, &a).unwrap_or_else(|| panic!("{a:?} asks"));
            assert!(message.contains("1 choice made by hand"), "{message}");
            if a == Action::LoadSamples {
                assert!(message.contains(") and your 1 choice"), "{message}");
            }
        }
        Action::ResetChoices.apply(&mut ws);
        assert!(ws.overrides.is_empty());
    }

    #[test]
    fn the_users_own_databases_are_protected() {
        let mut ws = samples();
        ws.using_samples = false;
        let (title, message, _) = question(&ws, &Action::LoadTimeseriesSamples).unwrap();
        assert!(title.contains("sample"));
        assert!(message.contains("(Workshop (sample), Catalogue (sample))"));
        assert!(question(&ws, &Action::Clear(Side::Source)).is_some());
        // Swapping without choices loses nothing.
        assert!(question(&ws, &Action::Swap).is_none());
        // An edited sample word list counts as work.
        let mut ws = samples();
        ws.word_list.push_str("\nfoo = bar\n");
        assert!(question(&ws, &Action::Open(Box::new(Workspace::new()))).is_some());
    }
}
