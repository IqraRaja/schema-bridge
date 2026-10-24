# Security and privacy

## What Schema Bridge processes

Only **structure exports**: table and column names, types, keys, links,
indexes, descriptions and estimated row counts. It never reads table
contents. Column descriptions and names can still be confidential (they may
describe a project or contain personal names), so treat exports and saved
comparisons like internal documents.

## Where processing happens

- The application is WebAssembly running in your browser. Loaded files are
  read by the browser and kept in memory; **nothing is uploaded**. There is no
  server-side component, account, analytics or telemetry.
- The development server (`dx serve`) only serves the application files to
  your own browser.
- The app makes no external requests: fonts, icons and styles are bundled.
- Nothing is stored in the browser between sessions; closing or reloading
  the page discards everything not saved to a file.

## Credentials and databases

- Schema Bridge has **no database connections** and never asks for a
  password. Exports are made by you, in your own tools, with read-only
  queries (a test checks that the queries only read).
- The draft SQL contains **placeholders** for connection options, never real
  values.
- Schema Bridge **never executes** SQL. The draft is text for review; it
  targets a test copy, opens a transaction and leaves `COMMIT` commented out.

## Files it writes

Reports (HTML, Markdown), CSV files, SQL files and the saved comparison
(JSON) are created as downloads on your computer. The HTML report is
self-contained and escapes all names, so a hostile name in an export cannot
inject script into it (tested). CSV fields that would start a spreadsheet
formula are prefixed with `'` (tested).

## Hosting a built copy

If you build the release version and serve it from a web server, the files
are still processed in each user's browser, but people must trust that
server not to have been altered. Serve it only from machines you control.

## Reporting a security problem

Please report security problems privately to the repository owner rather
than in a public issue.
