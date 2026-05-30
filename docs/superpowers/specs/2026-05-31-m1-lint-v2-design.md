# m1-lint v2 — Design Specification

**Date:** 2026-05-31
**Status:** Approved for implementation
**Scope:** v2 — JSON output, config file + threshold/filter flags, `--fix` autofix, two new CST-only rules (L010/L011)
**Spec (v1):** `docs/superpowers/specs/2026-05-30-m1-lint-v1-design.md`

> **Note:** Example identifiers and snippets in this document are synthetic
> placeholders, not drawn from any real project. The corpus path used by the
> integration tests is resolved via the `M1_CORPUS_PATH` env var (falling back
> to the sibling `m1-example` example project); corpus tests are skipped when it is
> unset.

---

## 1. Purpose

v1 shipped the `m1-lint` library + CLI with nine CST-only rules (L001–L009), a
`Rule` trait, a `Registry`, a `Runner` returning `RunResult`, and a
human-readable CLI output. v2 makes the linter consumable by editors and
configurable by projects, and adds a safe autofix mode — all without reaching
for the symbol model. Specifically, v2 delivers:

1. **`--format json`** structured output, so `m1-lsp` and editors can consume
   diagnostics directly.
2. **A `.m1lint.toml` config file** plus the `--max-line-length`,
   `--max-nesting-depth`, `--max-complexity` threshold flags and the
   `--select` / `--ignore` rule filters.
3. **`--fix`** autofix mode for the mechanically-unambiguous rules
   (L002, L003, L004, L005, L007), with a re-parse-and-verify safety guarantee
   that mirrors `m1-fmt`'s semantic-token-preservation invariant.
4. **Two new CST-only "A" rules** deferred from v1: **L010 tab-for-indentation**
   and **L011 comment-style** (`// ` spacing).

Everything here is computable from the `.m1scr` CST and raw source text alone.
Symbol-requiring "B" rules (naming conventions, single-assignment, absolute
paths, int/float mixing) remain out of scope and live in `m1-typecheck`.

`m1-lint` continues to depend only on `m1-core`; it must NOT import
`tree-sitter` directly. All CST access goes through `m1_core::Cst` and
`m1_core::Node`.

---

## 2. What v1 Already Provides (build on this, do not re-invent)

The v2 work extends the real v1 surface. The relevant existing items, with
their actual signatures:

- `m1_lint::diagnostic::LintCode` — a `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]`
  enum with variants `L001`..`L009` and a `Display` impl that prints `"L001"` etc.
- `m1_lint::diagnostic::LintDiagnostic { pub code: LintCode, pub inner: m1_core::Diagnostic }`
  plus `LintDiagnostic::new(code, range, byte_range, severity, message)`.
- `m1_lint::rules::Rule` trait with `code()`, `name()`, and the default-no-op
  hooks `check_file(&self, source, lines, diags)` and
  `check_node(&self, node, source, diags)`.
- `m1_lint::registry::Registry` with `empty()`, `register(Box<dyn Rule>)`,
  `default_v1()`, and `rules() -> &[Box<dyn Rule>]`.
- `m1_lint::runner::{Runner, RunResult}`. `Runner::new(Registry)`,
  `run_source(&str) -> RunResult`, `run_file(&Path) -> io::Result<RunResult>`.
  `RunResult { pub diagnostics: Vec<LintDiagnostic>, pub syntax_errors: Vec<m1_core::Diagnostic> }`.
  The runner already sorts diagnostics by `(line, column)`.
- `m1-core` API: `parse(&str) -> Cst`; `Cst::{source, root, syntax_diagnostics}`;
  `Node::{kind, kind_str, text, byte_range, range, is_error, is_missing, parent,
  children, named_children}`; `Kind` (the generated enum incl. `LineComment`,
  `BlockComment`, `SourceFile`, all operator tokens); `Diagnostic`, `Range`,
  `Position`, `Severity`, `Code`.

v2 adds new modules and a few new fields/constructors but does not break any of
the above. `default_v1()` is retained; a new `default_v2()` is added.

---

## 3. Key Decisions

### 3.1 Rule configurability — thresholds move from `const` to `RuleConfig`

In v1, L001/L008/L009 hard-code their thresholds as module `const`s
(`MAX_LEN = 88`, `MAX_DEPTH = 4`, `MAX_COMPLEXITY = 10`). v2 introduces a
`Config` struct that carries these thresholds and the active rule set. The three
threshold rules become **stateful structs** holding their limit:

```rust
pub struct LineTooLong { pub max_len: usize }
pub struct NestingTooDeep { pub max_depth: usize }
pub struct CyclomaticComplexity { pub max_complexity: u32 }
```

Their `Default` impls reproduce the v1 constants (88 / 4 / 10), so behaviour is
unchanged when no config is supplied. The other rules (L002–L007, L010, L011)
remain zero-sized.

### 3.2 Configuration precedence

Effective config is resolved in this order (later overrides earlier):

1. Built-in defaults (v1 constants; all rules enabled).
2. `.m1lint.toml` discovered by walking up from the linted file's directory to
   the filesystem root, stopping at the first `.m1lint.toml` found.
3. An explicit `--config <path>` flag (overrides discovery entirely).
4. CLI threshold flags (`--max-line-length`, `--max-nesting-depth`,
   `--max-complexity`).
5. CLI filter flags (`--select`, `--ignore`).

`--select` takes a comma-separated list of codes; when present, ONLY those
codes are active. `--ignore` removes codes from the active set. `--select` is
applied before `--ignore`. Config-file `select`/`ignore` keys behave the same
but are overridden by the CLI flags if both are present.

### 3.3 `.m1lint.toml` schema

```toml
# .m1lint.toml — all keys optional.
max-line-length   = 100      # default 88
max-nesting-depth = 5        # default 4
max-complexity    = 12       # default 10

# Exactly one of `select` / `ignore` is typical; if both given,
# `select` is applied first, then `ignore` subtracts from it.
select = ["L001", "L004", "L006"]   # if present, only these run
ignore = ["L007"]                   # remove these
```

Parsed with the `toml` crate into a `RawConfig` (all `Option`), then merged
into the effective `Config`. Unknown keys are a hard error (exit 2) to catch
typos early. Codes are parsed via a new `LintCode::from_code_str(&str) ->
Option<LintCode>`; an unknown code in `select`/`ignore` is exit 2.

### 3.4 JSON output format

`--format json` (the default remains `--format human`) emits a single JSON
object to stdout. Schema:

```json
{
  "version": 2,
  "files": [
    {
      "path": "Scripts/Demo.m1scr",
      "syntax_errors": [
        {
          "code": "syntax",
          "severity": "error",
          "message": "...",
          "range": { "start": {"line": 4, "column": 0}, "end": {"line": 4, "column": 1} },
          "byte_range": { "start": 40, "end": 41 }
        }
      ],
      "diagnostics": [
        {
          "code": "L004",
          "name": "eq-operator-preferred",
          "severity": "warning",
          "message": "use `eq` instead of `==`",
          "range": { "start": {"line": 11, "column": 6}, "end": {"line": 11, "column": 8} },
          "byte_range": { "start": 120, "end": 122 },
          "fixable": true
        }
      ]
    }
  ],
  "summary": { "errors": 1, "warnings": 1, "files": 1 }
}
```

- `line`/`column` are 0-based bytes (matching `m1_core::Position`); the
  consumer (m1-lsp) performs UTF-16 conversion. This mirrors m1-core's stated
  contract — m1-lint does NOT pre-convert to 1-based or to UTF-16.
- `severity` is the lowercase string form of `m1_core::Severity`.
- `fixable` is `true` when the rule can autofix this diagnostic (see §3.5).

**Decision: hand-rolled serialization, no `serde`.** The schema is small and
fixed; a `to_json` writer in a new `src/report.rs` keeps the dependency surface
minimal (only `toml` is added) and avoids `serde`/`serde_json`. Strings are
escaped (`"`, `\`, control chars) by a small helper. (If a future version needs
to *read* JSON, revisit; v2 only writes it.)

### 3.5 `--fix` autofix mode

`--fix` rewrites files in place, applying only **mechanically-unambiguous**
fixes. Fixable rules and their fixes:

| Code | Fix |
|------|-----|
| L002 trailing-whitespace | Delete the trailing whitespace run on each line |
| L003 missing-final-newline | Append a single `\n` |
| L004 `==`/`!=` | Replace operator token `==`→`eq`, `!=`→`neq` |
| L005 `&&`/`\|\|`/`!` | Replace `&&`→`and`, `\|\|`→`or`; **`!` only when safe** (see below) |
| L007 operator-spacing | Insert exactly one space on whichever side is missing |

A fix is modelled as an `Edit { byte_range: Range<usize>, replacement: String }`.
Rules that can fix opt in via a new **default-no-op** trait method:

```rust
fn fix_node(&self, node: &m1_core::Node, source: &str, edits: &mut Vec<Edit>) {}
fn fix_file(&self, source: &str, lines: &[&str], edits: &mut Vec<Edit>) {}
```

The `Fixer` collects edits from all enabled fixable rules in one pass, sorts
them by start offset, rejects any pair of **overlapping** edits (skips the
later one — fixed on a subsequent run), applies the non-overlapping edits
right-to-left so earlier offsets stay valid, and produces a candidate string.

**Safety guarantee (mirrors `m1-fmt`).** Before writing, the fixer:

1. Re-parses the candidate with `m1_core::parse`.
2. Asserts the candidate has **no new syntax errors**
   (`syntax_diagnostics().len()` does not increase vs. the original).
3. Asserts the **semantic token sequence is preserved** for the operator/logical
   replacements: the non-trivia token *kinds and texts* must match except where
   a flagged operator token was intentionally rewritten (`==`→`eq` is the only
   sanctioned kind change; whitespace-only and final-newline edits change no
   tokens at all).

If any check fails, the fix for that file is **abandoned** (file left
untouched) and a diagnostic is printed to stderr. This guarantees `--fix` never
emits broken or semantically-altered M1.

**`!` (L005 logical-not) caveat.** `!a` → `not a` requires inserting a space and
is only applied when a single space insertion yields a valid token boundary
(operand is not already separated). `!(...)`/`!ident` are safe; ambiguous cases
(e.g. `!!a`) are skipped and remain as a (non-fixable) diagnostic. L006
(float-eq) and L001/L008/L009 are **never** autofixed — the corrected form is a
semantic judgement (tolerance check / refactor), not a mechanical edit.

`--fix` applies only to rules that are *enabled* under the effective config, so
`--ignore L007 --fix` will not touch operator spacing.

### 3.6 Relationship to `m1-fmt` (no overlap, no conflict)

`m1-fmt` is a full reprinter: it discards all whitespace and re-emits canonical
spacing/indentation, preserving leaf-token text verbatim and guaranteeing the
non-whitespace/non-comment token sequence is byte-identical. There is a clear
division of labour:

- **Whitespace-shaped fixes overlap with `m1-fmt`.** L002 (trailing ws),
  L003 (final newline), and L007 (operator spacing) are all things `m1-fmt`
  already normalises. `m1-lint --fix` applies them as *minimal* edits (it does
  not reflow the file), which is the right behaviour when a user wants to fix a
  single lint without reformatting the whole file. When both tools run, the
  outcome converges: `m1-fmt` is a superset for whitespace. **Recommended
  pipeline:** run `m1-fmt` first, then `m1-lint --fix` for the
  token-substitution rules `m1-fmt` does *not* perform.
- **Token-substitution fixes are unique to `m1-lint`.** `m1-fmt` preserves the
  token sequence by contract and therefore will *never* rewrite `==`→`eq` or
  `&&`→`and`. These are stylistic/semantic-preference rewrites that only
  `m1-lint --fix` performs.

To avoid surprising users, the v2 docs and `--help` state: "`--fix` makes
minimal edits; for full canonical formatting use `m1-fmt`." `m1-lint` does not
attempt indentation reflow or any change `m1-fmt` owns beyond the three minimal
whitespace fixes above.

### 3.7 New rules L010 / L011

- **L010 tab-for-indentation.** A line whose leading whitespace (before the
  first non-whitespace char) contains a tab. File-level (`check_file`).
  Severity Warning. Fixable is **deferred to v3** (tab width is a config choice;
  not unambiguous), so L010 is a diagnostic-only rule in v2.
- **L011 comment-style.** A `LineComment` whose text starts with `//` but is not
  followed by a space or end-of-comment (i.e. `//foo` should be `// foo`).
  Node-level (`check_node`, `Kind::LineComment`). Severity Warning. **Fixable**:
  insert one space after `//`. Excludes shebang-style and `///`/`//!`-style
  lines? — M1 has no doc-comment convention, so only the bare `//x` case is
  flagged; `//` followed by another `/` (e.g. a commented-out `// // x`) is left
  alone to avoid touching deliberate separators (`////////`).

### 3.8 Exit codes (unchanged from v1, extended)

- `0` — no Error-severity diagnostics.
- `1` — at least one Error diagnostic (e.g. L006).
- `2` — invocation error: bad flag, unreadable file, malformed `.m1lint.toml`,
  unknown code in config/`--select`/`--ignore`.

With `--fix`, exit code reflects the diagnostics that **remain after fixing**
(re-lint the fixed buffer in-memory). A file abandoned by the safety check is
treated as a fix failure and contributes to a non-zero exit only if it still has
Error diagnostics.

---

## 4. Architecture

### 4.1 New / changed modules

```
src/
  lib.rs            (add: pub mod config; pub mod report; pub mod fix;)
  diagnostic.rs     (add L010, L011 + Display arms; add LintCode::from_code_str,
                     all_codes, name; add fixable())
  config.rs         (NEW: Config, RawConfig, load/discover/merge)
  report.rs         (NEW: human + json renderers)
  fix.rs            (NEW: Edit, Fixer, apply-with-verify)
  registry.rs       (add default_v2(); Registry::from_config(&Config))
  runner.rs         (RunResult gains nothing; Runner gains fix_source/fix_file)
  rules/mod.rs      (Rule trait gains default-no-op fix_node/fix_file;
                     add l010_*, l011_* modules)
  rules/l001_line_too_long.rs       (LineTooLong { max_len })
  rules/l008_nesting_too_deep.rs    (NestingTooDeep { max_depth })
  rules/l009_cyclomatic_complexity.rs (CyclomaticComplexity { max_complexity })
  rules/l002_trailing_whitespace.rs (add fix_file)
  rules/l003_missing_final_newline.rs (add fix_file)
  rules/l004_eq_operator_preferred.rs (add fix_node)
  rules/l005_logical_operator_preferred.rs (add fix_node)
  rules/l007_operator_spacing.rs    (add fix_node)
  rules/l010_tab_indentation.rs     (NEW)
  rules/l011_comment_style.rs       (NEW)
  main.rs           (full CLI rewrite: arg parsing, formats, --fix)
```

### 4.2 `Config`

```rust
pub struct Config {
    pub max_line_length: usize,    // default 88
    pub max_nesting_depth: usize,  // default 4
    pub max_complexity: u32,       // default 10
    /// The active rule set after select/ignore resolution.
    pub enabled: std::collections::BTreeSet<LintCode>,
}
```

`Config::default()` = v1 thresholds + all codes enabled. `Config::discover(path)`
walks up for `.m1lint.toml`; `Config::from_toml_str(&str)` parses; merge helpers
apply CLI overrides. `Registry::from_config(&Config)` registers exactly the
enabled rules, constructing the three threshold rules with the config values.

### 4.3 `Edit` and `Fixer`

```rust
pub struct Edit {
    pub byte_range: std::ops::Range<usize>,
    pub replacement: String,
}

pub struct Fixer<'a> { registry: &'a Registry }

impl<'a> Fixer<'a> {
    pub fn new(registry: &'a Registry) -> Self;
    /// Returns the fixed source, or None if no edits applied.
    /// Verifies re-parse + token preservation; returns Err if unsafe.
    pub fn fix_source(&self, source: &str) -> Result<Option<String>, FixError>;
}
```

`Runner` grows thin wrappers `fix_source`/`fix_file` that delegate to `Fixer`.

### 4.4 Token-preservation check

A private helper in `fix.rs`:

```rust
/// All non-trivia leaf tokens as (Kind, &str), in source order.
fn semantic_tokens(cst: &m1_core::Cst) -> Vec<(m1_core::Kind, String)>;
```

It walks the CST collecting leaf nodes (no children) whose kind is not
`LineComment`/`BlockComment`. The verifier compares original vs. fixed token
lists allowing exactly the sanctioned operator substitutions (`==`↔`eq`,
`!=`↔`neq`, `&&`↔`and`, `||`↔`or`, `!`↔`not`); all other tokens must match by
`(kind, text)`. Whitespace and final-newline edits leave the list identical.

---

## 5. Rule Specifications (new + changed)

### L010 — tab-for-indentation (NEW)

**Severity:** Warning · **Hook:** `check_file` · **Fixable:** no (v3)
**Description:** A line's leading whitespace contains a tab character.
**Message:** `tab character in indentation; use spaces`
**Range:** the leading-whitespace span containing the tab(s) on that line.
**Notes:** Only the *indentation* (leading run) is checked; tabs after the first
non-whitespace char are not L010 (they may be inside strings/comments). Empty or
all-whitespace lines: flag only if the whitespace contains a tab.

### L011 — comment-style (NEW)

**Severity:** Warning · **Hook:** `check_node` (`Kind::LineComment`) · **Fixable:** yes
**Description:** A line comment's text is `//x…` with no space after `//`.
**Message:** `add a space after \`//\``
**Fix:** insert one space at the byte offset just after `//`.
**Notes:** Not flagged: `// x`, bare `//`, and `//` immediately followed by `/`
(separators like `////`). The check reads `node.text()` and inspects bytes 2..3.

### L001 / L008 / L009 (CHANGED — configurable thresholds)

Behaviour identical to v1 when defaults are used. The structs now carry their
limit and read it instead of a module `const`. Messages still interpolate the
*effective* limit (`max {n}`), so a config of `max-line-length = 100` produces
`line is 105 characters (max 100)`.

### L002 / L003 / L004 / L005 / L007 (CHANGED — add fixes)

Detection logic unchanged. Each gains a `fix_*` hook emitting `Edit`s per §3.5.
The detection (`check_*`) and fix (`fix_*`) share helper functions so they never
diverge.

---

## 6. CLI (v2)

```
m1-lint [OPTIONS] <file>...

OPTIONS:
  --format <human|json>      output format (default: human)
  --fix                      apply safe autofixes in place
  --config <path>            use this .m1lint.toml (skip discovery)
  --max-line-length <N>
  --max-nesting-depth <N>
  --max-complexity <N>
  --select <CODES>           comma-separated; only these rules run
  --ignore <CODES>           comma-separated; remove these rules
  -h, --help
  -V, --version
```

Argument parsing stays dependency-free (manual, matching v1's `main.rs` style):
a small loop over `args[1..]` separating flags from file paths. `--format json`
prints the JSON object (§3.4) to **stdout**; human format keeps v1's stderr
lines. `--fix` and `--format json` together: emit JSON describing the
*remaining* diagnostics after fixing.

---

## 7. Testing Strategy

Extends v1's three layers (per-rule unit tests, `tests/fixtures.rs`,
`tests/corpus.rs`); all existing tests must keep passing.

- **Config unit tests** (`src/config.rs`): TOML parse, unknown-key error,
  select/ignore resolution, threshold merge, discovery walk-up (using a temp
  dir).
- **JSON unit tests** (`src/report.rs`): a fixed `RunResult` renders to the
  exact expected JSON; string escaping for `"`/`\`/newline.
- **Fix unit tests** (`src/fix.rs` + each fixable rule): each rule's fix on a
  minimal snippet produces the expected output; overlapping-edit rejection;
  the safety verifier rejects a hand-crafted unsafe edit.
- **`--fix` idempotency:** `fix(fix(x)) == fix(x)` on fixture inputs.
- **Token preservation:** for every fix fixture, `semantic_tokens(before)` and
  `semantic_tokens(after)` match modulo sanctioned substitutions.
- **New rule fixtures:** `tests/fixtures/l010_tabs.{m1scr,diag}`,
  `tests/fixtures/l011_comment.{m1scr,diag}`.
- **Fix fixtures:** `tests/fixtures_fix/<name>.in.m1scr` +
  `<name>.out.m1scr`; a new `tests/fix.rs` runner asserts `fix(in) == out`.
- **Corpus:** extend `tests/corpus.rs` with a `corpus_fix_safe` test asserting
  `--fix` over every corpus file re-parses clean and preserves semantic tokens
  (skipped without `M1_CORPUS_PATH`).

---

## 8. Out of Scope / Deferred to v3

| Item | Reason |
|------|--------|
| Magic-number detection | High false-positive risk without type/context; needs the symbol/type model (lives in m1-typecheck territory) |
| `stdin` pipe mode (`-`) | Editor pipe-mode; defer until m1-lsp drives it |
| L001 autofix (wrap long lines) | No unambiguous wrap point; reflow is a formatter concern |
| L010 autofix (tabs→spaces) | Tab width is a config choice; not a single unambiguous edit |
| Per-rule severity overrides in config | Adds schema surface; no demand yet |
| Excessive-/multiple-space operator-spacing fix | v2 only fixes *missing* spaces (the common case); collapsing extra spaces overlaps m1-fmt |
| `--fix` for L006 (float-eq) | The fix is a tolerance-check refactor, not mechanical |
| `serde`-based JSON (read path) | v2 only writes JSON; hand-rolled writer suffices |

---

## 9. Non-Goals (unchanged from v1)

- m1-lint does not resolve channel types, project configs, or cross-file deps.
- m1-lint does not implement LSP; that is `m1-lsp`'s job (it calls
  `Runner::run_source` and now consumes `--format json` / the `report` module).
- `--fix` does not reformat; full canonical formatting is `m1-fmt`'s job.
