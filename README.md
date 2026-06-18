# m1-lint

A linter for the MoTeC M1 script language (`.m1scr`). It runs style and
correctness rules over the parsed syntax tree — naming, layout, complexity,
operator conventions, and the code-layout rules the M1 Development Manual
specifies. It knows nothing about the project's symbol model (that is
[m1-typecheck](https://github.com/C-Nucifora/m1-typecheck)'s job). It is both
a **library** (consumed by `m1-lsp` as a diagnostic source) and a **CLI**.

## Install

Prebuilt binaries for Linux, macOS, and Windows are attached to each
[release](https://github.com/C-Nucifora/m1-lint/releases). Or build from
source:

```sh
cargo install --git https://github.com/C-Nucifora/m1-lint.git --tag <latest>
```

## Usage

```sh
m1-lint .                            # lint every .m1scr under a directory
m1-lint --fix file.m1scr             # apply safe autofixes in place
m1-lint --fix --diff file.m1scr      # preview the fixes, write nothing
m1-lint --format sarif Scripts/      # SARIF for GitHub code scanning
m1-lint --rules                      # the full rule catalogue
m1-lint --explain L022               # one rule's rationale and fix behaviour
```

The rule catalogue is self-documenting: `--rules` lists every rule (add
`--format json` for a machine-readable version sourced from the same enum the
linter runs), and `--explain <CODE>` prints a rule's full rationale. Autofixes
are only applied when the fixed source re-parses and preserves the script's
semantic tokens — a fix can never change meaning.

The defaults follow the M1 Development Manual (tab indentation, layout and
naming conventions); deviation is a config choice, not the default. A few
deliberately noisy rules are opt-in — see `--rules`.

## Rules

The catalogue is also available at runtime via `m1-lint --rules` (and
`--rules --format json`, the machine-readable single source of truth). Each
rule's SARIF `helpUri` deep-links to its heading below, so a GitHub code-scanning
alert's "View rule" link lands on the right entry.

### line-too-long (L001)

Line exceeds the configured maximum length. Severity: warning.

### trailing-whitespace (L002)

Trailing whitespace at end of line. Severity: warning · auto-fixable (`--fix`).

### missing-final-newline (L003)

File does not end with a newline. Severity: warning · auto-fixable (`--fix`).

### eq-operator-preferred (L004)

Prefer `eq`/`neq` over `==`/`!=`. Severity: warning · auto-fixable (`--fix`).

### logical-operator-preferred (L005)

Prefer the spelled logical operators (and/or/not). Severity: warning · auto-fixable (`--fix`).

### float-eq-comparison (L006)

Float compared with an equality operator. Severity: error.

### operator-spacing (L007)

Missing space around an operator. Severity: warning · auto-fixable (`--fix`).

### nesting-too-deep (L008)

Block nesting exceeds the configured depth. Severity: warning.

### cyclomatic-complexity (L009)

Cyclomatic complexity exceeds the configured ceiling. Severity: warning.

### indentation-style (L010)

Indentation does not match the configured style. Severity: warning.

### comment-style (L011)

`//` comment needs a space after the slashes. Severity: warning · auto-fixable (`--fix`).

### unused-local (L012)

Local binding is never used. Severity: warning.

### expand-undefined-variable (L014)

Expand body references an undefined $(VAR). Severity: warning.

### local-missing-initializer (L015)

Local declared without an initializer. Severity: warning.

### local-variable-naming (L016)

Local name does not follow the naming convention. Severity: warning.

### magic-number (L017)

Unnamed numeric literal (magic number). Severity: warning.

### semicolon-spacing (L018)

Incorrect spacing around a semicolon. Severity: warning · auto-fixable (`--fix`).

### cognitive-complexity (L019)

Cognitive complexity exceeds the configured ceiling. Severity: warning.

### object-naming (L020)

Object names begin with an uppercase letter. Severity: warning.

### one-statement-per-line (L021)

Write only one statement per line. Severity: warning · auto-fixable (`--fix`).

### keyword-paren-spacing (L022)

Put a space between a keyword and its parenthesis. Severity: warning · auto-fixable (`--fix`).

### call-paren-spacing (L023)

No space between a function name and its parenthesis. Severity: warning · auto-fixable (`--fix`).

### ternary-condition-parens (L024)

Wrap a ternary condition in parentheses. Severity: warning · auto-fixable (`--fix`).

### local-scope-too-wide (L025)

Local declared in a wider scope than its uses need. Severity: warning.

### top-level-indentation (L026)

Top-level code begins in the first column. Severity: warning · auto-fixable (`--fix`).

### file-final-blank-line (L027)

Function/method script ends with a blank line. Severity: warning · auto-fixable (`--fix`).

### brace-style (L028)

Braces follow the configured style (default Allman, manual p.65: "a separate line
for each brace"). The opening `{` must be on its own line; a K&R `if (a) {` is
flagged. Set `[format] brace_style = "kr"` (shared with m1-fmt) to flip it.
Severity: warning · fix by running m1-fmt.

### indentation-depth (L029)

A nested block is indented one indentation level per enclosing block (manual
p.65: "indent conditional block by one tab stop"). Flags a nested statement
indented by too few or too many levels — e.g. an `if` body left at column 1, or
indented three tabs where one is expected. Complements L010 (which judges only
the indent character) and L026 (column-1 top-level code); when the indentation
uses the wrong character it defers to L010 rather than double-flagging.
**Opt-in** (`--select L029`): like L017 and L027 it ships off by default because
the real corpora keep house-style layout it flags en masse. Measured in the
configured `[format] indent_style` (shared with m1-fmt). Severity: warning · fix
by running m1-fmt.

### clause-parentheses (L030)

Wrap each comparison sub-clause of a compound logical expression in parentheses
(manual p.65, "use parentheses to clarify clauses in an expression"; its worked
example is `if ((a > b) and (b < c))`). Flags a relational or equality
comparison (`>`, `<`, `eq`, `neq`, …) that is an operand of a logical `and`/`or`
and is not already parenthesized — so `a > b and b < c` is flagged twice and
fixed to `(a > b) and (b < c)`. Symbolic `&&`/`||` are left to L005 (which
prefers the keyword spellings). **Opt-in** (`--select L030`): like L017, L027 and
L029 it ships off by default because the real corpora write compound booleans
unparenthesized. Severity: warning · auto-fixable (`--fix`).

## Configuration and workflow

Rule selection, thresholds, and indent style live in a `.m1lint.toml`
discovered upward from the input (or the `[lint]` section of a workspace
`m1-tools.toml`, shared with the other tools — see the
[m1-tools configuration docs](https://github.com/C-Nucifora/m1-tools#configuration)).
CLI flags override both.

- **Suppression in source:** `// @m1:allow(L0xx)` on a construct — the M1
  analogue of `// eslint-disable-next-line`.
- **Per-rule severity:** a `[severity]` table promotes or softens individual
  rules (`L001 = "error"`) without forking the rule set.
- **Baselines:** `--write-baseline` snapshots current findings so later runs
  with `--baseline` report only new regressions — the adoption path for a
  legacy codebase.

## Development

The CI gate is `cargo test`, `cargo clippy --all-targets -- -D warnings`, and
`cargo fmt --all -- --check`. The corpus test lints every `.m1scr` under
`$M1_CORPUS_PATH` (falling back to a sibling `m1-example/` checkout) and
asserts the linter never panics; it skips if no corpus is present. Example
identifiers in docs and fixtures are synthetic placeholders, not drawn from
any real project.

## License

GPL-3.0-or-later — see [LICENSE](LICENSE).

## Trademark

Independent, community-built open-source tooling for the MoTeC® M1 script
language. Not affiliated with, authorised, or endorsed by MoTeC Pty Ltd.
"MoTeC" and "M1" are trademarks of MoTeC Pty Ltd.
