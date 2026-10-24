//! The two browser actions the app needs: copying text to the clipboard and
//! saving text as a file. Both stay on the user's computer; nothing is sent
//! anywhere.

use dioxus::prelude::*;

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
    let _ = eval.send((file_name.to_string(), media_type.to_string(), text));
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
                    state.set(Some(ok));
                }
            },
            {shown}
        }
    }
}
