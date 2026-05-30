# m1-lint v1 — Design Specification

**Date:** 2026-05-30
**Status:** Draft
**Scope:** v1 — CST-only lint rules, CLI + library crate

---

## 1. Purpose

`m1-lint` is a static linter for the MoTeC M1 script language (`.m1scr`). It reads source files, parses them via `m1-core`, walks the resulting CST, and emits structured diagnostics for style violations, dangerous idioms, and complexity thresholds defined in the project's CONTRIBUTING guidelines.

`m1-lint` occupies the third layer of the toolchain:

```
tree-sitter-m1  (grammar)
      ↓
m1-core         (parse, CST, diagnostics)
      ↓
m1-lint         (lint rules)     ← this crate
m1-fmt          (formatting)
m1-typecheck    (type checking)
m1-lsp          (language server)
```

`m1-lint` depends on `m1-core` and must NOT import `tree-sitter` directly. All CST access goes through `m1_core::Cst` and `m1_core::Node`.

---

## 2. Key Decisions

### 2.1 Crate structure

`m1-lint` is a single Cargo workspace member with two public surfaces:

- **`m1_lint` library** (`src/lib.rs`) — the `Rule` trait, `Registry`, `Runner`, and all rules. This is the surface consumed by `m1-lsp` and downstream tools.
- **`m1-lint` binary** (`src/main.rs`) — a thin CLI wrapper that calls the library and formats output.

### 2.2 Diagnostic code strategy

`m1-core`'s `Code` enum currently defines `SyntaxError` and `MissingToken`. Lint codes have a different provenance (style, convention) and version lifecycle from parse codes. Adding lint codes to m1-core would couple the two crates incorrectly.

**Decision:** m1-lint defines its own `LintCode` enum (e.g. `L001`, `L002`, …). Diagnostics emitted by lint rules use `m1_core::Diagnostic` as the struct (reusing `range`, `byte_range`, `severity`, `message`) but carry the code as a `String` rather than `m1_core::Code`. A newtype `LintDiagnostic` wraps `m1_core::Diagnostic` and adds `LintCode`. The runner returns `Vec<LintDiagnostic>`.

This keeps m1-core stable and lets m1-lint add/remove codes freely.

### 2.3 Output format

v1 ships one output format: **human-readable**, mimicking the rustc/cargo style:

```
path/to/file.m1scr:12:5: warning[L002]: use `eq` instead of `==`
```

`--format json` is deferred to v2 (needed when feeding results into m1-lsp or editors that consume LSP diagnostics directly).

### 2.4 Rule invocation model

Rules are **visitor-style**: the runner does a single depth-first CST walk and calls each rule's `check_node` method for every node. Some rules also receive the full source text as a `&str` and a pre-split `&[&str]` line slice for line-level checks. This avoids walking the tree multiple times.

A separate `check_file` hook on the `Rule` trait is called once per file (before the walk) for rules that operate purely on raw text (trailing whitespace, line length, final newline).

### 2.5 Configuration

v1: no configuration file. All thresholds are compile-time constants matching the CONTRIBUTING spec:
- max line length: **88** (measured after `rstrip`, i.e. trailing whitespace is stripped before counting)
- max nesting depth: **4**
- max cyclomatic complexity per function/when block: **10**

A `--max-line-length`, `--max-nesting-depth`, `--max-complexity` CLI flag set is planned for v2, together with a `.m1lint.toml` config file.

### 2.6 Exit codes

- `0` — no diagnostics at Error severity
- `1` — at least one Error diagnostic
- `2` — invocation error (bad flag, file not found)

---

## 3. Architecture

### 3.1 Rule trait

```rust
pub trait Rule: Send + Sync {
    /// Short machine-readable code, e.g. "L001".
    fn code(&self) -> LintCode;

    /// Human-readable name, e.g. "line-too-long".
    fn name(&self) -> &'static str;

    /// Called once per file before the CST walk.
    /// `lines` is the source split on `\n` (no trailing newline on each element).
    fn check_file(
        &self,
        source: &str,
        lines: &[&str],
        diags: &mut Vec<LintDiagnostic>,
    ) {}

    /// Called for every node in the CST (depth-first pre-order).
    fn check_node(
        &self,
        node: &m1_core::Node,
        source: &str,
        diags: &mut Vec<LintDiagnostic>,
    ) {}
}
```

Default (no-op) implementations let each rule implement only the hook(s) it needs.

### 3.2 Registry

```rust
pub struct Registry {
    rules: Vec<Box<dyn Rule>>,
}

impl Registry {
    pub fn default_v1() -> Self { /* registers all v1 rules */ }
    pub fn register(&mut self, rule: Box<dyn Rule>);
    pub fn rules(&self) -> &[Box<dyn Rule>];
}
```

### 3.3 Runner

```rust
pub struct Runner {
    registry: Registry,
}

impl Runner {
    pub fn new(registry: Registry) -> Self;

    pub fn run_source(&self, source: &str) -> RunResult;
    pub fn run_file(&self, path: &Path) -> RunResult;
}

pub struct RunResult {
    pub diagnostics: Vec<LintDiagnostic>,
    pub syntax_errors: Vec<m1_core::Diagnostic>,
}
```

The runner:
1. Calls `m1_core::parse(source)` → `Cst`.
2. Collects `cst.syntax_diagnostics()` into `RunResult::syntax_errors`.
3. Splits `cst.source()` into lines.
4. Calls `rule.check_file(source, lines, &mut diags)` for each rule.
5. Walks the CST depth-first; calls `rule.check_node(node, source, &mut diags)` for each node.
6. Returns the `RunResult`.

### 3.4 LintDiagnostic

```rust
#[derive(Debug, Clone)]
pub struct LintDiagnostic {
    pub code: LintCode,
    pub inner: m1_core::Diagnostic,
}
```

### 3.5 LintCode enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LintCode {
    L001, // line-too-long
    L002, // trailing-whitespace
    L003, // missing-final-newline
    L004, // eq-operator-preferred  (== → eq, != → neq)
    L005, // logical-operator-preferred  (&& → and, || → or, ! → not)
    L006, // float-eq-comparison
    L007, // operator-spacing
    L008, // nesting-too-deep
    L009, // cyclomatic-complexity
}
```

---

## 4. Rule Classification: A/B Split

### What makes a rule "A" (v1-eligible)

A rule is computable from the `.m1scr` CST and raw source text alone, without any knowledge of the symbol model (channel kinds, parameter types, project configuration).

### What makes a rule "B" (deferred)

A rule requires one or more of:
- **Symbol identity**: knowing whether a name is a local variable, channel reference, parameter, or constant (these have different naming conventions: `lowerCamelCase` locals, `UpperCamelCase` parameters, `ALL_CAPS` constants, `l` prefix on locals).
- **Project configuration**: `.m1prj` / `.m1cfg` files that define available channels and their types.
- **Cross-file analysis**: e.g. single-assignment-per-channel must trace all writes to a channel name across a compilation unit.
- **Type information**: e.g. determining whether two operands of `eq` are float-typed requires type inference, not just syntactic pattern matching. (The float-eq heuristic in L006 is a best-effort CST-only approximation.)

### Classification table

| Rule | Category | v1? | Reason if deferred |
|------|----------|-----|-------------------|
| Max line length (88, post-rstrip) | A | YES | Pure text |
| Trailing whitespace | A | YES | Pure text |
| Missing final newline | A | YES | Pure text |
| `==` / `!=` instead of `eq` / `neq` | A | YES | Token identity |
| `&&` / `\|\|` / `!` instead of `and` / `or` / `not` | A | YES | Token identity |
| Float literal compared with `eq` / `==` (heuristic) | A | YES | CST pattern |
| Operator spacing (no space around `+`, `-`, `*`, `/`, etc.) | A | YES | Token/whitespace |
| Nesting depth (if/when/for > 4 deep) | A | YES | CST depth walk |
| Cyclomatic complexity (branches per function/when) | A | YES | CST branch count |
| Local variable naming (`l` prefix + camelCase) | B | NO | Needs symbol kind |
| Parameter naming (`UpperCamelCase`) | B | NO | Needs symbol kind |
| Constant naming (`ALL_CAPS`) | B | NO | Needs symbol kind |
| Single assignment per channel | B | NO | Needs channel identity + cross-statement analysis |
| Absolute-path references only | B | NO | Needs project/channel model |
| Magic numbers (no unnamed numeric literals) | B | deferred | Context-dependent; many false positives without type info; deferred to v2 |
| Comment spacing (`// ` with a space after `//`) | A | v2 | Low priority; deferred to keep v1 tight |
| No tabs for indentation | A | v2 | Low priority; deferred |
| Int/float mixing in arithmetic | B | NO | Needs type inference |

**v1 rules: L001–L009** (9 rules).

---

## 5. v1 Rule Specifications

### L001 — line-too-long

**Severity:** Warning
**Hook:** `check_file`
**Description:** A line, after stripping trailing whitespace (`rstrip`), exceeds 88 characters.
**Message:** `line is {n} characters (max 88)`
**Source:** `check_line_length.py` (max=88, uses rstrip before measuring)
**Notes:**
- Measure `line.trim_end().len()` in bytes (m1scr is ASCII; Unicode not expected but handle gracefully with char count fallback).
- The threshold 88 matches the existing Python oracle exactly.

### L002 — trailing-whitespace

**Severity:** Warning
**Hook:** `check_file`
**Description:** A line ends with one or more space or tab characters.
**Message:** `trailing whitespace`
**Notes:** Report the range of the trailing whitespace (from the last non-whitespace char to end of line).

### L003 — missing-final-newline

**Severity:** Warning
**Hook:** `check_file`
**Description:** The file does not end with a newline character (`\n`).
**Message:** `file does not end with a newline`
**Notes:** Report at the last byte of the file.

### L004 — eq-operator-preferred

**Severity:** Warning
**Hook:** `check_node`
**Description:** The binary operators `==` or `!=` are used; the project prefers `eq` and `neq` respectively.
**Message:** `use \`eq\` instead of \`==\`` / `use \`neq\` instead of \`!=\``
**Source:** CONTRIBUTING.md "Comparisons" section.
**CST node:** `BinaryExpression` with child token `Kind::EqEq` or `Kind::BangEq`.
**Notes:** Report the range of the operator token, not the whole expression.

### L005 — logical-operator-preferred

**Severity:** Warning
**Hook:** `check_node`
**Description:** The logical operators `&&`, `||`, or `!` are used; the project prefers `and`, `or`, and `not`.
**Message:** `use \`and\` instead of \`&&\`` / `use \`or\` instead of \`||\`` / `use \`not\` instead of \`!\``
**Source:** CONTRIBUTING.md "Comparisons" section.
**CST node:** `BinaryExpression` with `Kind::AmpAmp` / `Kind::PipePipe`; `UnaryExpression` with `Kind::Bang`.
**Notes:** Report the range of the operator token.

### L006 — float-eq-comparison

**Severity:** Error
**Hook:** `check_node`
**Description:** A float literal appears as a direct operand of `eq` or `==` (or `neq`/`!=`). Floating-point equality comparisons are unreliable.
**Message:** `never compare floats with equality operators; use a tolerance check`
**Source:** CONTRIBUTING.md "Comparisons: never compare floats with `==`".
**CST pattern:** `BinaryExpression` whose operator is `EqEq`, `BangEq`, `eq`, or `neq` (i.e. a call-expression `eq(...)` with `Identifier` "eq"), AND at least one direct child is a `Number` node whose text contains `.` or `e`/`E` (float notation), OR whose text is the result of a known float-returning call (heuristic: skip call expressions — too complex for v1; only flag literal floats).
**Notes:** This is a best-effort heuristic. It will miss `eq(a, b)` where `a` is a float variable. False negatives are acceptable; false positives must not occur for integer literals.

### L007 — operator-spacing

**Severity:** Warning
**Hook:** `check_node`
**Description:** A binary operator token is not surrounded by exactly one space on each side.
**Message:** `missing space around operator \`{op}\``
**Source:** CONTRIBUTING.md "Calculations: use spaces around operators".
**CST node:** `BinaryExpression`; check that the byte immediately before and after the operator token is a space character.
**Operators checked:** `+`, `-`, `*`, `/`, `%`, `=` (assignment), `<`, `>`, `<=`, `>=`.
**Notes:**
- `eq`, `neq`, `and`, `or`, `not` are word tokens; spacing is handled by their natural token boundaries.
- Unary minus/plus are excluded (they have no left-hand operand).
- In v1 only check for absence of any space (most common violation); don't check for excessive spaces.

### L008 — nesting-too-deep

**Severity:** Warning
**Hook:** `check_node`
**Description:** A control-flow construct (`if`, `when`, `for`, `while`) is nested more than 4 levels deep.
**Message:** `nesting depth {n} exceeds maximum of 4`
**Source:** `check_complexity.py` (max nesting threshold).
**Notes:**
- Track current depth as a mutable counter threaded through the walk, OR compute depth by walking `node.parent()` chain at each control node (simpler, slightly less efficient; fine for v1).
- Counting: each `IfStatement`, `WhenStatement`, `ForStatement`, `WhileStatement` increments the depth.

### L009 — cyclomatic-complexity

**Severity:** Warning
**Hook:** `check_node`
**Description:** The cyclomatic complexity of a function or when-block exceeds 10 (number of branches + 1).
**Message:** `cyclomatic complexity {n} exceeds maximum of 10`
**Source:** `check_complexity.py`.
**Notes:**
- Complexity = 1 + number of decision points: each `if`, `elseif`, `when`, `for`, `while`, `and`/`&&`, `or`/`||` within the function/when body.
- Only emit this diagnostic at the function/when declaration node, not at each branch.
- Report the function/when node's range (first line of declaration).
- If the language has no explicit function declarations (scripts are top-level), compute complexity over the top-level `source_file` node. Clarify with owner (see Open Questions).

---

## 6. Diagnostics Reporting (CLI)

### Human-readable format (v1)

One line per diagnostic:

```
<file>:<line>:<col>: <severity>[<code>]: <message>
```

Line and column are 1-based. Example:

```
Scripts/AMS.m1scr:47:1: warning[L001]: line is 92 characters (max 88)
Scripts/AMS.m1scr:53:14: warning[L004]: use `eq` instead of `==`
Scripts/AMS.m1scr:112:5: error[L006]: never compare floats with equality operators; use a tolerance check
```

Syntax errors (from `cst.syntax_diagnostics()`) are printed first, prefixed with `error[syntax]:`, then lint diagnostics sorted by line.

A summary line at the end:

```
2 errors, 5 warnings in 3 files
```

### JSON format (`--format json`) — deferred to v2

---

## 7. Testing Strategy

### 7.1 Per-rule unit tests

Each rule module (`src/rules/l001_line_too_long.rs`, etc.) has an inline `#[cfg(test)]` module. Tests use small M1 script snippets as string literals, run through the `Runner`, and assert:

- Exact `LintCode` emitted
- Exact line/column of each diagnostic
- No false positives on conforming code

Example pattern:

```rust
#[test]
fn detects_eq_eq() {
    let source = "x = a == b\n";
    let runner = Runner::new(Registry::default_v1());
    let result = runner.run_source(source);
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].code, LintCode::L004);
    assert_eq!(result.diagnostics[0].inner.range.start.line, 0);
}

#[test]
fn no_false_positive_eq_function() {
    let source = "x = eq(a, b)\n";
    let runner = Runner::new(Registry::default_v1());
    let result = runner.run_source(source);
    assert!(result.diagnostics.iter().all(|d| d.code != LintCode::L004));
}
```

### 7.2 Corpus smoke test

A `tests/corpus.rs` integration test:
1. Reads all `*.m1scr` files under `../m1-example/UQR-EV/01.00/Scripts/` (path from env var `M1_CORPUS_PATH`, test skipped if unset).
2. Asserts the runner does not panic on any file.
3. Asserts the runner returns cleanly (no `Err`) on each file.
4. A curated baseline file (`tests/corpus_baseline.json`) records the expected diagnostic counts per file; the test asserts these counts don't change. The baseline is generated by running `cargo test -- --include-ignored generate_baseline` on a known-good corpus.

### 7.3 Regression fixtures

`tests/fixtures/` contains pairs of `.m1scr` (input) and `.diag` (expected diagnostics in `line:col:code:message` format). The fixture runner iterates all pairs and compares. These are the canonical acceptance tests for each rule and are committed with the rule implementation.

---

## 8. Out of Scope / Deferred

The following are explicitly deferred from v1:

### Deferred (B) — requires symbol model

| Rule | Reason |
|------|--------|
| Local variable naming (`l` prefix + `lowerCamelCase`) | Needs symbol kind: must know if identifier is a local declaration |
| Parameter naming (`UpperCamelCase`) | Needs symbol kind: parameter vs local vs channel |
| Constant naming (`ALL_CAPS`) | Needs symbol kind |
| Single assignment per channel | Needs channel identity + full-file assignment tracking |
| Absolute-path channel references | Needs project model and channel resolution |
| Int/float arithmetic mixing | Needs type inference |

### Deferred (A) — lower priority, v2

| Rule | Reason for deferral |
|------|-------------------|
| `--format json` output | Useful for editor integration; not critical for CLI use in v1 |
| `.m1lint.toml` config file | No user-facing threshold customisation needed in v1 |
| `--fix` auto-fix mode | Non-trivial; the formatter (`m1-fmt`) may subsume some of these |
| Tab-for-indentation check | Low frequency in corpus; deferred |
| Comment style (`// ` vs `//`) | Low priority; deferred |
| Magic number detection | High false-positive risk without type/context info |
| `--select` / `--ignore` rule filters | v2 alongside config file |

---

## 9. Open Questions for the Owner

1. **Function declaration nodes in the CST:** Does the M1 grammar define explicit function/procedure declaration nodes (analogous to `function_definition` in other grammars)? If so, what is the `Kind` name? L009 (cyclomatic complexity) needs to know the scoping node to count branches per function. If the language has no explicit function declarations and all logic lives at top level inside `when` blocks, complexity should be computed per `WhenStatement`. Please confirm.

2. **`eq` / `neq` / `and` / `or` / `not` as built-in call expressions vs keywords:** The task description says the project *prefers* `eq/neq/and/or/not` over `==/!=/&&/||/!`. Are `eq`, `neq`, `and`, `or`, `not` parsed as `Identifier` nodes (i.e. function calls in the CST) or as distinct keyword `Kind` variants? This determines whether L004/L005 must check for their presence (for the "correct usage" test) or only flag their absence. Please confirm the `Kind` values for these tokens.

3. **Operator spacing for assignment (`=`) vs equality (`eq`/`==`):** CONTRIBUTING says "use spaces around operators". Does this apply to the channel-assignment operator `=` as well as arithmetic operators? Assignment spacing is almost always correct in real code but the rule should be explicit. Also, does spacing apply to `:=` (if that token exists)?

4. **Should L006 (float-eq) be an Error or a Warning?** The CONTRIBUTING rule says "never compare floats with `==`", suggesting it is a hard violation. However, since it is a heuristic (literal floats only), a false negative rate exists. Treating it as Error may be aggressive. Owner should confirm severity preference before implementation.

---

## 10. Non-Goals

- m1-lint does not modify source files (that is `m1-fmt`'s job).
- m1-lint does not resolve channel types, project configurations, or cross-file dependencies.
- m1-lint does not implement a language server protocol; that is `m1-lsp`'s job. However, the library crate is designed so `m1-lsp` can call `Runner::run_source` directly.
- m1-lint v1 does not support `stdin` input (deferred to v2 for editor pipe-mode).
