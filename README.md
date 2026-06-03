# m1-lint

A linter for the MoTeC M1 script language (`.m1scr`). It runs CST-only style and
correctness rules over a parsed script — it knows nothing about the project's
symbol model (that is `m1-typecheck`'s job). It is both a **library** (consumed by
`m1-lsp` as a diagnostic source) and a **CLI**.

## Workspace layout

The M1 toolchain lives in **six separate repositories**. They are not published
to crates.io; instead each crate pins its upstreams as **versioned git-tag Cargo
dependencies** (e.g. `m1-core = { git = "…/m1-core.git", tag = "v0.3.1" }`), so
this crate **does** build from a standalone clone — Cargo fetches its upstreams
from their tagged releases. Checking the whole set out as siblings under one
parent directory is handy for cross-repo work, but is not required to build:

```
<parent>/
├── tree-sitter-m1/   # grammar (root)
├── m1-core/          # parse / CST / diagnostics
├── m1-lint/          # this crate
├── m1-fmt/           # formatter
├── m1-typecheck/     # type checker
└── m1-lsp/           # language server; depends on the four above
```

**`m1-lint` depends on `m1-core`** (a git-tag dep that transitively pulls in
`tree-sitter-m1`), and is itself a git-tag dependency of `m1-lsp`. (The
`m1-example` corpus project, used by the corpus test, is an optional sibling
checkout.)

Because every dependency is pinned by tag, the coupling **is** visible on
GitHub — each `Cargo.toml` names its upstreams and their versions, and
Dependabot opens bump PRs as new upstream tags ship. Cutting a new upstream
release and bumping `tag = "vX.Y.Z"` in each consumer is what propagates a
change across the stack.

## Rules

| Code | Severity | Rule |
|------|----------|------|
| L001 | Warning | line too long (configurable threshold) |
| L002 | Warning | trailing whitespace |
| L003 | Warning | missing final newline |
| L004 | Warning | prefer `eq` over `==` |
| L005 | Warning | prefer the spelled logical operators (`and`/`or`/`not`) |
| L006 | Error   | float compared with an equality operator |
| L007 | Warning | missing space around an operator |
| L008 | Warning | nesting too deep (configurable threshold) |
| L009 | Warning | cyclomatic complexity too high (configurable threshold) |
| L010 | Warning | indentation uses the wrong char (default: tabs required) |
| L011 | Warning | comment-style violation |
| L012 | Warning | local binding is never used |
| L014 | Warning | `expand` variable `$(VAR)` not bound by an enclosing `expand` |
| L015 | Warning | `local` declaration has no initializer (M1 requires one) |
| L016 | Warning | `local` name not lowercase-initial / contains spaces |
| L017 | Warning | magic number (unnamed numeric literal in an expression) — **opt-in**, enable with `--select L017` |

The catalogue is also available machine-readably with `m1-lint --rules --format
json` (schema `{"version":1,"rules":[{"code","name","fixable"}]}`), sourced
directly from the `LintCode` enum so external tools can enumerate the rules
without copying the list.

## CLI usage

```sh
m1-lint <file.m1scr>...              # human-readable diagnostics
m1-lint --format json <file.m1scr>   # machine-readable JSON
m1-lint --fix <file.m1scr>           # apply safe autofixes in place
m1-lint --rules                      # list every rule (add --format json)
```

Autofixes are only applied when the fixed source re-parses and preserves the
script's semantic tokens. Rule selection and thresholds can be configured via a
`.m1lint.toml` discovered upward from the input file (or passed explicitly), with
`select`/`ignore` lists and per-rule thresholds. `indent-style` chooses the
indentation L010 enforces — `"tab"` (default, per the M1 manual) or `"spaces"`.

## Build & test

```sh
cargo build --release      # binary at target/release/m1-lint
cargo test                 # unit + corpus + fixture + autofix-acceptance tests
```

The corpus test runs every `.m1scr` under `$M1_CORPUS_PATH` (falling back to the
sibling `m1-example` example project) and asserts the linter never panics; it is
skipped if the directory is absent.

## Note on examples

Example identifiers in the docs and fixtures are **synthetic placeholders**, not
drawn from any real project.

## License

Not yet chosen — decided by the repository owner. Treated as proprietary until
then.

## License

Licensed under the GNU General Public License v3.0 or later (GPL-3.0-or-later) — see [LICENSE](LICENSE).

Copyright (C) 2026 The M1 Tools authors.
