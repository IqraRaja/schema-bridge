# Troubleshooting

## Installing and running

**`dx: command not found`.** The Dioxus CLI is not installed or not on the
`PATH`. Run `cargo binstall dioxus-cli@0.7.10 --force`, then open a new
terminal. On macOS/Linux check that `~/.cargo/bin` is on the `PATH`
(`source "$HOME/.cargo/env"`); on Windows reopen PowerShell.

**`dx --version` shows 0.6 or 0.8.** Schema Bridge uses Dioxus 0.7; install
`dioxus-cli@0.7.10` as above.

**"can't find crate for `core`" / "the `wasm32-unknown-unknown` target may not be installed".**
Run `rustup target add wasm32-unknown-unknown`.

**`error: linker 'link.exe' not found` (Windows).** Install the Visual Studio
Build Tools with "Desktop development with C++", then reopen PowerShell.

**Port 8080 is in use.** `dx serve --package schema-bridge-app --port 8081`,
then open http://localhost:8081.

**The first build is slow.** It downloads and compiles all dependencies
(several minutes). Later builds take seconds.

**The page is blank or styles look old after an update.** Reload the page
with the cache bypassed (Shift+Reload / Ctrl+F5). If it stays blank, open the
browser's developer console and look for errors; stop and restart `dx serve`.

**`cargo test` at the root does not build the app.** That is intended: the
workspace's default member is the library. Check the app with
`cargo check -p schema-bridge-app --target wasm32-unknown-unknown`.

## Loading files

**"This file is not a Schema Bridge export."** The first lines do not match
any known export. Check that you saved the result of a query from
`export-queries/` as **CSV** (not Excel), with the header row (or without
it, but not with a different one). In SSMS turn on "Include column headers"
before saving.

**Columns appear in one cell / the file is not recognised (European Excel).**
Files with `;` as separator are accepted; if you edited the file in Excel,
save it again as "CSV UTF-8".

**Missing parts (chips not ticked).** The older four-file export needs all
four files; add them one by one to the same database. The one-file export
ticks all chips at once.

**Text with line breaks looks broken.** Make sure your tool quotes fields
that contain separators or line breaks (SSMS: "Quote strings containing list
separators when saving .csv results").

**Unknown types are reported as "Special type".** The type name is not in
Schema Bridge's vocabulary; comparisons for that column are by name only.
Please report the type name.

## Results

**Few tables are matched automatically.** The names differ too much. Add
words (`client = customer`), whole column names (`cust_no = customer_id`) or
table pairs (`table: kunde = customer`) to the word list, or choose the
target table by hand.

**A target link column is never filled automatically.** By design: filling a
link from a plain value needs a lookup, so choose it by hand. The draft SQL
then writes a lookup marked `TODO`.

**My choices disappeared.** Loading or clearing a file, swapping source and
target, or changing a table's target drops the choices that no longer apply.
Reloading the page discards everything; use **Save comparison**.

**"This workspace was saved by a newer version".** Update Schema Bridge
(`git pull`) and open the file again.

**Copy does not work.** Some browsers block clipboard access outside
`localhost` or without a user action. The button then says so; select the
text and press ⌘C / Ctrl+C.

**Nothing happens on Save.** Your browser may block downloads from the page;
allow downloads for localhost.

## Getting help

Open an issue in the repository with: what you did, what you expected, what
happened, your operating system and browser, and `dx --version`. Never attach
exports of real databases unless they may be shared; a small made-up example
that shows the problem is best.
