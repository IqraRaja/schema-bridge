# Contributing

Thank you for helping with Schema Bridge. This page is the development
workflow: setup, checks, tests and conventions.

## Setup

Install the tools as in [docs/installation.md](docs/installation.md), then:

```sh
cargo test                                 # library tests
dx serve --package schema-bridge-app       # the application, reloading on save
```

Read [docs/architecture.md](docs/architecture.md) first: all logic lives in
`crates/schema-bridge-core` (no UI, fully tested); the Dioxus application in
`crates/schema-bridge-app` only displays it.

## Before every change is shared

All of these must pass:

```sh
cargo fmt --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy -p schema-bridge-app --target wasm32-unknown-unknown -- -D warnings
dx build --package schema-bridge-app --release
```

Then try the change in the running application (`dx serve`), in both themes
and on a narrow window if it touches the UI.

## Tests

- **Every new behaviour gets a test of the behaviour itself**, in the core
  library: a unit test next to the code (`#[cfg(test)]`) for small parts, an
  integration test in `crates/schema-bridge-core/tests/` for whole workflows.
- Tests use small hand-built schemas or the made-up samples in `demo/`.
  **Never commit a real organisation's schema** or any data from it.
- Tests need no database, network or credentials.

Test files worth knowing:

| File | Protects |
| --- | --- |
| `tests/import_parity.rs`, `checks_parity.rs`, `types_parity.rs`, `matching_parity.rs`, `plan_parity.rs` | Results equal to the original prototype's on the samples (fixtures in `tests/fixtures/`) |
| `tests/review.rs` | Confidence, reasons, review states, accept/remove/override, search, sorting |
| `tests/user_guide.rs` | Every number quoted in the user guide's walk-through |
| `tests/large_schema.rs` | 400 × 400 tables analysed quickly |
| `tests/timeseries.rs` | Time-series schemas end to end: the Plant → Readings sample, `TS-*` and `MAP-TS-*` rules, matching, retention, SQL, report, saved comparison (numbers quoted in docs/time-series.md) |

If a parity fixture must change because behaviour changed on purpose, record
why in `docs/decisions.md` and regenerate the fixture as that decision
describes. If `tests/user_guide.rs` fails after an intended change, update
the guide in the same change.

## Conventions

- **Plain language** in everything users read: findings, notes, reasons,
  documentation. Say what it means and what to do.
- **Database-neutral core**: anything specific to one database system goes
  into `crates/schema-bridge-core/src/systems.rs`
  ([adding a database](docs/adding-a-database.md)).
- **Design tokens only** in `assets/main.css`: no colours or sizes outside
  the token list; semantic colours only for status.
- **Never claim support that does not exist**: keep README and docs in step
  with the code in the same change.
- Code comments explain *why*, at the density of the surrounding code.
- Minimum Rust version is 1.85 (`rust-version` in `Cargo.toml`); do not use
  newer language features such as let-chains.

## Documentation

User documentation is in `docs/` and the README; developer documentation in
`docs/architecture.md`, `docs/adding-a-database.md` and `docs/decisions.md`.
A change that alters behaviour updates both the user-facing and the
developer-facing page. Decisions that make the implementation differ from
`docs/specification.md` are added to its last section.

## Changelog

Add a line under "Unreleased" in [CHANGELOG.md](CHANGELOG.md) for every
user-visible change.

## License

Schema Bridge is released under the [MIT License](LICENSE). By contributing,
you agree that your contributions are licensed under the same terms.
