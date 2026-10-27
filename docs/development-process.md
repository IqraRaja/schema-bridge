# How Schema Bridge is developed

This page connects the documents that describe how Schema Bridge was
specified, planned, built, tested and released. Each section says where the
details are.

## 1. Purpose

Schema Bridge is a schema analysis, compatibility assessment and
migration/integration planning tool. It reads structure exports of a source
and a target database, reports findings, suggests mappings for review, lists
open decisions, plans a test load and drafts SQL. It does not move data and
does not choose the target design ([decision 27](decisions.md#27-an-analysis-and-planning-tool-not-a-migration-tool)).

## 2. Origin: the browser prototype

Schema Bridge started as a single-file browser prototype. The Rust
application reproduces its workflow, terms and behaviour, module by module,
and parity tests compare the results with the prototype's on the sample
schemas ([decision 1](decisions.md#1-the-browser-prototype-is-the-reference),
[decision 6](decisions.md#6-parity-tests-against-the-prototype)).

## 3. Specification

[specification.md](specification.md) was written on 22 October 2026, before
the Rust implementation started. It holds the requirements (FR-01 …),
supported databases, architecture, data model, analysis rules, plan and SQL,
interface, security, quality and risks. Its text is kept as written; where
the implementation differs, the difference is listed at its end.

## 4. Plan and milestones

[development-plan.md](development-plan.md) divides the work into phases and
small milestones, and its *Progress* section records each milestone's state:
milestones 1–8 up to release 0.1.0, and the steps after it (CI-1, UI-1, UI-2,
terminology, CI-2, positioning). Each milestone is checked with the full set
of checks before the next one starts, and its documentation is updated with
it.

## 5. Decisions

[decisions.md](decisions.md) records the important technical and design
choices and why they were made, newest last. When a decision changes, its
entry is updated rather than deleted. Larger features were decided first and
implemented second (for example decision 24 for time series).

## 6. Architecture

[architecture.md](architecture.md) describes the two crates (the core
library without UI code, and the Dioxus application), the data flow, the
modules, the import boundary and the extension points.
[adding-a-database.md](adding-a-database.md) shows how a database system is
added.

## 7. Testing

- **Core library** (`crates/schema-bridge-core`): unit tests next to the code
  and integration tests in `tests/`: parity with the prototype, matching
  review, time series, a large-schema timing test, and a test that keeps the
  user guide's quoted numbers true. No test needs a database, network or
  credentials.
- **Application** (`crates/schema-bridge-app`): unit tests for its own logic
  (readiness, confirmations, the error card, the plan's design notes), run
  natively with `cargo test -p schema-bridge-app`.
- **In the browser:** changes to the interface are tried in the running
  application, in both themes and at narrow widths.

Details: [CONTRIBUTING.md](../CONTRIBUTING.md#tests).

## 8. CI/CD and releases

- **Branches:** feature branch → pull request into `develop` → pull request
  into `main` → release tag on `main`.
- **CI** (`.github/workflows/ci.yml`): every push to `develop` or `main` and
  every pull request into either runs format, check and Clippy, the core tests
  on Linux, macOS and Windows, the application's tests and WebAssembly
  checks, and the `dx` release build.
- **Releases** (`.github/workflows/release.yml`): publishing a GitHub release
  for a tag on `main` builds the web application, starts it in a headless
  browser, and attaches the archive and `SHA256SUMS.txt`.

Details: [CONTRIBUTING.md](../CONTRIBUTING.md#branches-ci-and-releases),
[decision 26](decisions.md#26-branches-ci-and-releases).

## 9. History

- [CHANGELOG.md](../CHANGELOG.md): every user-visible change, per release.
- The Git history begins with release 0.1.0 (the first commits contain the
  milestones up to 0.1.0, tagged `v0.1.0`); the later steps follow as their
  own commits. For the milestones before 0.1.0, the progress table in
  [development-plan.md](development-plan.md) and the decisions are the
  record.
- [docs/images](images): screenshots of the application, refreshed when the
  interface changes.
