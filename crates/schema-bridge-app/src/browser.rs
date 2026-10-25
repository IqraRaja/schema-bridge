//! The browser actions the app needs: copying text to the clipboard, saving
//! text as a file, showing a stage or section, guarding against stray file
//! drops, and remembering the colour theme. Everything stays on the user's computer; nothing is sent anywhere.
//! The theme is the only thing kept in the browser's storage.

use dioxus::prelude::*;

use crate::feedback::notify;

/// Where the chosen colour theme is kept in the browser's local storage.
const THEME_KEY: &str = "schema-bridge.theme";

/// Copies text to the clipboard. Returns false when the browser refused
/// (for example without permission), so the caller can say so.
pub async fn copy_text(text: String) -> bool {
    let mut eval = document::eval(
        r#"
        const text = await dioxus.recv();
        try { await navigator.clipboard.writeText(text); dioxus.send(true); }
        catch (e) { dioxus.send(false); }
        "#,
    );
    if eval.send(text).is_err() {
        return false;
    }
    eval.recv::<bool>().await.unwrap_or(false)
}

/// Offers text as a file download with the given name and media type.
pub fn download_text(file_name: &str, media_type: &str, text: String) {
    let eval = document::eval(
        r#"
        const [name, type, text] = await dioxus.recv();
        const url = URL.createObjectURL(new Blob([text], { type }));
        const a = document.createElement("a");
        a.href = url;
        a.download = name;
        document.body.appendChild(a);
        a.click();
        a.remove();
        setTimeout(() => URL.revokeObjectURL(url), 1000);
        "#,
    );
    if eval
        .send((file_name.to_string(), media_type.to_string(), text))
        .is_ok()
    {
        notify(format!("Saved {file_name}"));
    }
}

/// Waits the given number of milliseconds.
pub async fn wait_ms(ms: u32) {
    let mut eval = document::eval(
        r#"
        const ms = await dioxus.recv();
        setTimeout(() => dioxus.send(true), ms);
        "#,
    );
    if eval.send(ms).is_ok() {
        let _ = eval.recv::<bool>().await;
    }
}

/// After a stage change: scrolls to the top and moves the keyboard focus to
/// the stage's heading, so screen readers announce the new stage. Waits two
/// frames so the new stage is on screen first.
pub fn reveal_stage() {
    document::eval(
        r#"
        requestAnimationFrame(() => requestAnimationFrame(() => {
            window.scrollTo(0, 0);
            const heading = document.querySelector("main.content h2");
            if (heading) {
                heading.setAttribute("tabindex", "-1");
                heading.focus({ preventScroll: true });
            }
        }));
        "#,
    );
}

/// Scroll behaviour that respects the user's reduced-motion setting.
const SCROLL: &str =
    r#"(matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth")"#;

/// Opens "How to make an export file" and scrolls to it.
pub fn reveal_how_to() {
    document::eval(&format!(
        r#"
        const how = document.getElementById("how-to-export");
        if (how) {{
            how.open = true;
            how.scrollIntoView({{ behavior: {SCROLL}, block: "start" }});
            how.querySelector("summary")?.focus({{ preventScroll: true }});
        }}
        "#
    ));
}

/// Scrolls to the two database panels and puts the keyboard focus on the
/// source panel's file chooser.
pub fn reveal_panels() {
    document::eval(&format!(
        r#"
        document.querySelector(".bench")?.scrollIntoView({{ behavior: {SCROLL}, block: "start" }});
        document.getElementById("file-source")?.focus({{ preventScroll: true }});
        "#
    ));
}

/// Keeps a file dropped outside the database panels from replacing the
/// application with the file (the browser's default), which would lose the
/// comparison. Only drags that carry files are affected.
pub fn guard_file_drops() {
    document::eval(
        r#"
        if (!window.__schemaBridgeDropGuard) {
            window.__schemaBridgeDropGuard = true;
            const files = (e) => e.dataTransfer && Array.from(e.dataTransfer.types).includes("Files");
            window.addEventListener("dragover", (e) => { if (files(e)) e.preventDefault(); });
            window.addEventListener("drop", (e) => { if (files(e)) e.preventDefault(); });
        }
        "#,
    );
}

/// The colour theme chosen in an earlier session, if the browser kept it.
pub async fn saved_theme() -> Option<String> {
    let mut eval = document::eval(
        r#"
        const key = await dioxus.recv();
        let value = null;
        try { value = window.localStorage.getItem(key); } catch (e) {}
        dioxus.send(value);
        "#,
    );
    eval.send(THEME_KEY).ok()?;
    eval.recv::<Option<String>>().await.ok().flatten()
}

/// Remembers the colour theme; does nothing where storage is blocked.
pub fn save_theme(value: &str) {
    let eval = document::eval(
        r#"
        const [key, value] = await dioxus.recv();
        try { window.localStorage.setItem(key, value); } catch (e) {}
        "#,
    );
    let _ = eval.send((THEME_KEY, value.to_string()));
}

/// A file name made from a database name: `Workshop (sample)` → `workshop-sample`.
pub fn file_stem(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "schema".into() } else { out }
}

/// A "Copy" button that says whether copying worked.
#[component]
pub fn CopyButton(text: String, label: String) -> Element {
    let mut state = use_signal(|| None::<bool>);
    // A new text resets the feedback.
    let mut last = use_signal(|| text.clone());
    if *last.peek() != text {
        last.set(text.clone());
        state.set(None);
    }
    let shown = match state() {
        Some(true) => "Copied ✓".to_string(),
        Some(false) => "Copy failed: select the text and press ⌘C / Ctrl+C".to_string(),
        None => label,
    };
    rsx! {
        button {
            class: "btn secondary",
            aria_live: "polite",
            onclick: move |_| {
                let text = text.clone();
                async move {
                    let ok = copy_text(text).await;
                    if ok {
                        notify("Copied to the clipboard");
                    }
                    state.set(Some(ok));
                }
            },
            {shown}
        }
    }
}
