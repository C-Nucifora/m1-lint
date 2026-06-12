# AGENTS.md — m1-lint

Guidance for coding agents working in this repository.

## Purpose

The style/correctness linter for M1 scripts. CST-only by design: it sees one
parsed file at a time and nothing of the project symbol model — any check
needing to know what a name *resolves to* belongs in `m1-typecheck`, not here.
`m1-lsp` consumes it as a library, so rule changes surface in editors, not
just the CLI.

## Things that are deliberate (don't "fix" them)

- **Fix safety is the contract.** An autofix is applied only if the fixed
  source re-parses and preserves the semantic token sequence. Never ship a
  fixer without that verification, however mechanical the edit looks.
- **Manual defaults, configurable deviation.** The M1 Development Manual's
  layout/naming rules (tabs, first-column code, naming case, …) are the
  defaults; house-style deviation is a config option. When unsure whether a
  rule's behaviour is "correct", the manual wins over current output —
  several rules cite manual pages in their rationale.
- **Noise policy is part of rule design.** Rules real corpora violate en
  masse (magic numbers, final-blank-line) are opt-in; default-on rules must
  run clean on a well-formed project. Check a candidate rule against a real
  corpus before defaulting it on.
- **Don't fight the formatter.** A layout rule that `m1-fmt` would undo (or
  vice versa) is a bug in the pairing — check what the formatter does with
  the "fixed" output before adding a layout rule or fixer.
- **The rule catalogue is generated from the `LintCode` enum** (`--rules`,
  consumed by telescope-m1.nvim and others). Adding a rule means the enum
  metadata, the implementation, `--explain` text, and tests — there is no
  separate hand-maintained list to update, and that's intentional.

## Heads-up for downstream

A release that adds rules breaks telescope-m1.nvim's sync test (it downloads
the latest lint binary and compares rule lists) — sync its `rules.lua` in the
same cascade.

## Build / test gate

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

CI also runs rustdoc with `-D warnings`, a security audit, and an MSRV job.
The MSRV pin in CI (`dtolnay/rust-toolchain@<version>`) must stay in sync with
`rust-version` in `Cargo.toml` — never bump one without the other.

The corpus test needs `$M1_CORPUS_PATH` (or a sibling `m1-example/`) and
skips when absent.

## Dependencies and releases

Depends on `m1-core` and `m1-workspace` via **versioned git tags** — never
`branch`/`path`/`[patch]`; the repo must build exactly like a public clone,
and everything in one lockfile must pin the same m1-core tag. This is a
binary repo: a version bump on `main` makes `release.yml` tag it and upload
prebuilt binaries. After releasing, open the consumer bump PRs (`m1-lsp`, the
`m1-ci` tool-version pin, and the telescope rules sync when rules changed)
immediately rather than waiting for Dependabot.
