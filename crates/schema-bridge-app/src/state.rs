//! Values derived from the workspace, computed once per change and shared
//! with every screen through the context. With large schemas the analysis
//! is the expensive part, so no screen computes it on its own.

use dioxus::prelude::*;
use schema_bridge_core::Finding;
use schema_bridge_core::workspace::{Analysis, Side, Workspace};

#[derive(Clone, Copy)]
pub struct Derived {
    /// Matching, integration findings and load order; `None` until both
    /// databases are loaded.
    pub analysis: Memo<Option<Analysis>>,
    /// Problems inside each database.
    pub findings: Memo<Vec<(Side, Finding)>>,
}

/// Creates the shared values; called once by the application.
pub fn provide(workspace: Signal<Workspace>) -> Derived {
    let analysis = use_memo(move || workspace.read().analysis());
    let findings = use_memo(move || workspace.read().findings());
    use_context_provider(|| Derived { analysis, findings })
}

/// The shared values, in any screen.
pub fn derived() -> Derived {
    use_context::<Derived>()
}
