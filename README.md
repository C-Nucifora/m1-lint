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
| L009 | Warning | cyclomatic complexity too high — total decision points (configurable, default 40) |
| L010 | Warning | indentation uses the wrong char (default: tabs required) |
| L011 | Warning | comment-style violation |
| L012 | Warning | local binding is never used |
| L014 | Warning | `expand` variable `$(VAR)` not bound by an enclosing `expand` |
| L015 | Warning | `local` declaration has no initializer (M1 requires one) |
| L016 | Warning | `local` name not lowercase-initial / contains spaces |
| L017 | Warning | magic number (unnamed numeric literal in an expression) — **opt-in**, enable with `--select L017` |
| L018 | Warning | space in front of a `;` (manual: "Do not put spaces in front of semicolons") — fixable |
| L019 | Warning | cognitive complexity too high — Sonar-style, nesting-weighted (configurable, default 15) |
| L020 | Warning | object name begins lowercase (manual p.64: objects begin with an uppercase letter; locals are L016's side) |
| L021 | Warning | more than one statement on a line (manual p.65, the first layout rule) |
| L022 | Warning | no space between a keyword and `(` (`if(a)`) — fixable |
| L023 | Warning | space between a function name and `(` (`Func (a)`) — fixable |
| L024 | Warning | ternary condition not in parentheses (manual p.67: `(condition) ? a : b`) — fixable |
| L025 | Warning | `local` only used inside one nested block deeper than its declaration (manual p.67: most constrained scope); static locals, call initializers and expand bodies exempt |

`L009` (cyclomatic) counts every decision point equally and so grows with sheer
size; `L019` (cognitive) is nesting-weighted, so deeply-nested logic costs far
more than a long flat sequence. They are complementary: L019 is the primary
readability gate (`--max-cognitive-complexity`), L009 is a loose backstop for
pathological branch counts (`--max-complexity`).

The catalogue is also available machine-readably with `m1-lint --rules --format
json` (schema `{"version":2,"rules":[{"code","name","severity","fixable","summary"}]}`), sourced
directly from the `LintCode` enum so external tools can enumerate the rules
without copying the list.

### Suppressing a diagnostic — `// @m1:allow(...)`

A `// @m1:allow(L0xx, …)` annotation (the toolchain-wide annotation framework,
m1-core#33) suppresses the listed rule(s) on the construct it is attached to —
the M1 analogue of `// eslint-disable-next-line`. A bare `// @m1:allow`
suppresses every rule on that construct.

```m1
// @m1:allow(L010)
    Indented With Spaces = 1;     // L010 not reported on this line

Foo = some_long_expression; // @m1:allow(L008)   ← trailing form, attaches to this statement
```

The annotation attaches **leading** (the next statement, so it stacks on
consecutive lines above its target) or **trailing** (a statement it follows on
the same line). Suppression is line-scoped to the target construct. (Reporting
only — `--fix` still applies mechanical fixes; most suppressible rules are
non-fixable.)

### Per-rule severity overrides

`.m1lint.toml` accepts a `[severity]` table mapping codes to
`error|warning|info|hint`, applied after the rule runs — promote `L001` to a
hard error in CI, or soften `L006`, without forking the rule set:

```toml
[severity]
L001 = "error"
```

### Baselines

`--write-baseline FILE` snapshots every current finding (anchored on the
offending line's content, not its number, so unrelated edits don't resurrect
suppressed findings); later runs with `--baseline FILE` report only new
regressions. The adoption path for a legacy codebase.

## CLI usage

```sh
m1-lint <file.m1scr>...              # human-readable diagnostics
m1-lint --format json <file.m1scr>   # machine-readable JSON
m1-lint --fix <file.m1scr>           # apply safe autofixes in place
m1-lint --rules                      # list every rule (add --format json)
m1-lint --explain L022               # one rule's full rationale and fix behaviour
m1-lint --fix --diff <file.m1scr>    # preview --fix as a unified diff (writes nothing)
m1-lint --format sarif <files>       # SARIF 2.1.0 for GitHub code scanning etc.
m1-lint --write-baseline .m1lint-baseline.json <files>   # snapshot current findings
m1-lint --baseline .m1lint-baseline.json <files>         # report only NEW findings
```

Flags accept both the space-separated (`--format json`) and the GNU
`--flag=value` (`--format=json`, `--select=L010`) forms.

Autofixes are only applied when the fixed source re-parses and preserves the
script's semantic tokens. Rule selection and thresholds can be configured via a
`.m1lint.toml` discovered upward from the input file (or passed explicitly), with
`select`/`ignore` lists and per-rule thresholds. `indent-style` chooses the
indentation L010 enforces — `"tab"` (default, per the M1 manual) or `"spaces"`.
Keys may be written kebab-case (`max-line-length`) or snake_case
(`max_line_length`), so the unified `m1-tools.toml` / `m1-lsp --scaffold-config`
output can be used directly as a `.m1lint.toml`.

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

Licensed under the GNU General Public License v3.0 or later (GPL-3.0-or-later) — see [LICENSE](LICENSE).

Copyright (C) 2026 The M1 Tools authors.

## Trademark

Independent, community-built open-source tooling for the MoTeC® M1 script
language. Not affiliated with, authorised, or endorsed by MoTeC Pty Ltd.
"MoTeC" and "M1" are trademarks of MoTeC Pty Ltd.
