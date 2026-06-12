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
