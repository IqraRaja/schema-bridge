//! Writes the comparison report for the sample schemas, so the report can
//! be seen without the application:
//!
//! ```sh
//! cargo run -p schema-bridge-core --example sample_report > report.html
//! cargo run -p schema-bridge-core --example sample_report -- --markdown > report.md
//! ```

use schema_bridge_core::report::{report_html, report_markdown};
use schema_bridge_core::workspace::Workspace;

fn main() {
    let mut ws = Workspace::new();
    ws.load_samples();
    let analysis = ws.analysis();
    let markdown = std::env::args().any(|a| a == "--markdown");
    let text = if markdown {
        report_markdown(&ws, analysis.as_ref())
    } else {
        report_html(&ws, analysis.as_ref())
    };
    print!("{text}");
}
