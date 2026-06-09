//! m1-lint — command-line linter for the MoTeC M1 script language.

use std::path::PathBuf;
use std::process;

use clap::{Parser, ValueEnum};

use m1_lint::config::Config;
use m1_lint::registry::Registry;
use m1_lint::report;
use m1_lint::runner::Runner;

/// Output format for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    Human,
    Json,
    Sarif,
}

#[derive(Parser, Debug)]
#[command(
    name = "m1-lint",
    version,
    about = "Linter for the MoTeC M1 script language",
    after_help = "--fix makes minimal edits; for full canonical formatting use m1-fmt."
)]
struct Args {
    /// Files to lint
    files: Vec<PathBuf>,

    /// Output format
    #[arg(long, value_enum, default_value_t = Format::Human)]
    format: Format,

    /// Apply safe autofixes in place
    #[arg(long)]
    fix: bool,

    /// Use this .m1lint.toml
    #[arg(long, value_name = "path")]
    config: Option<PathBuf>,

    /// Maximum line length (L001)
    #[arg(long, value_name = "N")]
    max_line_length: Option<usize>,

    /// Maximum nesting depth (L008)
    #[arg(long, value_name = "N")]
    max_nesting_depth: Option<usize>,

    /// Cyclomatic complexity ceiling (L009)
    #[arg(long, value_name = "N")]
    max_complexity: Option<u32>,

    /// Cognitive complexity ceiling (L019)
    #[arg(long, value_name = "N")]
    max_cognitive_complexity: Option<u32>,

    /// Comma-separated codes; only these rules run
    #[arg(long, value_name = "CODES")]
    select: Option<String>,

    /// Comma-separated codes; remove these rules
    #[arg(long, value_name = "CODES")]
    ignore: Option<String>,

    /// Print the rule catalogue (with --format json) and exit
    #[arg(long)]
    rules: bool,

    /// Explain one rule (rationale, manual reference, fix behaviour) and exit
    #[arg(long, value_name = "CODE")]
    explain: Option<String>,

    /// With --fix: print the unified diff of what would change, write nothing.
    /// Exits 1 when the diff is non-empty.
    #[arg(long)]
    diff: bool,

    /// Suppress findings recorded in this baseline file (see --write-baseline)
    #[arg(long, value_name = "FILE")]
    baseline: Option<PathBuf>,

    /// Lint, then write all current findings to FILE as the new baseline
    #[arg(long, value_name = "FILE")]
    write_baseline: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();

    // --rules prints the rule catalogue (the single source of truth for tools
    // that enumerate rules) and exits, honouring --format json|human.
    if args.rules {
        match args.format {
            Format::Json => println!("{}", report::render_rules_json()),
            Format::Human | Format::Sarif => print!("{}", report::render_rules_human()),
        }
        process::exit(0);
    }

    // --explain CODE: print the rule rationale and exit (#108).
    if let Some(code_str) = &args.explain {
        match m1_lint::diagnostic::LintCode::from_code_str(code_str.trim()) {
            Some(code) => {
                println!("{}", report::explain(code));
                process::exit(0);
            }
            None => fail(&format!("unknown lint code `{code_str}` (see --rules)")),
        }
    }

    if args.files.is_empty() {
        fail("no input files");
    }

    // Split a `--select`/`--ignore` comma list into trimmed, non-empty codes.
    let select = args.select.as_deref().map(split_codes);
    let ignore = args.ignore.as_deref().map(split_codes);

    let mut any_error = false;
    let mut json_files: Vec<(String, m1_lint::runner::RunResult)> = Vec::new();

    // Baseline handling (#111): load the suppression set up front; collect a
    // fresh one when --write-baseline is given.
    let baseline = match &args.baseline {
        Some(p) => match m1_lint::baseline::Baseline::load(p) {
            Ok(b) => Some(b),
            Err(e) => fail(&format!("could not read baseline {}: {e}", p.display())),
        },
        None => None,
    };
    let mut new_baseline = args
        .write_baseline
        .as_ref()
        .map(|_| m1_lint::baseline::Baseline::default());
    let mut any_diff = false;

    for path in &args.files {
        // Resolve config, lowest layer first: the unified m1-tools.toml, then the
        // tool-specific file (explicit --config, else discovered .m1lint.toml /
        // user-global), then CLI flags. So a project can be configured entirely
        // from m1-tools.toml; a .m1lint.toml still works and overrides it.
        let dir = m1_lint::config::dir_of(path);
        let mut cfg = Config::default();
        if let Some(tc) = m1_workspace::config::M1ToolsConfig::discover(&dir)
            && let Err(e) = cfg.apply_tools_config(&tc)
        {
            cfg_fail(e);
        }
        match &args.config {
            Some(p) => {
                let text = std::fs::read_to_string(p)
                    .unwrap_or_else(|e| fail(&format!("could not read {}: {e}", p.display())));
                if let Err(e) = cfg.apply_toml_str(&text) {
                    cfg_fail(e);
                }
            }
            None => {
                if let Err(e) = cfg.apply_discovered_file(&dir) {
                    cfg_fail(e);
                }
            }
        }
        if let Some(n) = args.max_line_length {
            cfg.max_line_length = n;
        }
        if let Some(n) = args.max_nesting_depth {
            cfg.max_nesting_depth = n;
        }
        if let Some(n) = args.max_complexity {
            cfg.max_complexity = n;
        }
        if let Some(n) = args.max_cognitive_complexity {
            cfg.max_cognitive_complexity = n;
        }
        if let Err(e) = cfg.apply_filters(select.clone(), ignore.clone()) {
            cfg_fail(e);
        }

        // Skip files matching an `exclude` glob from the config (#9).
        if cfg.is_excluded(path) {
            continue;
        }

        let runner = Runner::new(Registry::from_config(&cfg));

        match runner.run_file(path) {
            Ok(mut result) => {
                let display = path.display().to_string();
                // The baseline anchors on line content, so it needs the source.
                if baseline.is_some() || new_baseline.is_some() {
                    let source = std::fs::read_to_string(path).unwrap_or_default();
                    if let Some(b) = &baseline {
                        b.filter(&display, &source, &mut result.diagnostics);
                    }
                    if let Some(nb) = &mut new_baseline {
                        nb.record(&display, &source, &result.diagnostics);
                    }
                }
                if !result.syntax_errors.is_empty() {
                    any_error = true;
                }
                if result
                    .diagnostics
                    .iter()
                    .any(|d| d.inner.severity == m1_core::Severity::Error)
                {
                    any_error = true;
                }
                match args.format {
                    Format::Human => {
                        eprint!("{}", report::render_human(&display, &result));
                    }
                    Format::Json | Format::Sarif => json_files.push((display, result)),
                }

                // --diff (#112): preview what --fix would change; never write.
                if args.diff {
                    let source = std::fs::read_to_string(path).unwrap_or_default();
                    match runner.fix_source_stable(&source) {
                        Ok(Some(fixed)) => {
                            print!(
                                "{}",
                                m1_workspace::diff::unified_diff(
                                    &path.display().to_string(),
                                    &source,
                                    &fixed
                                )
                            );
                            any_diff = true;
                        }
                        Ok(None) => {}
                        Err(e) => {
                            eprintln!(
                                "warning: could not compute fixes for {}: {e}",
                                path.display()
                            );
                            any_error = true;
                        }
                    }
                }

                // Apply fixes only after linting completed without an I/O
                // error. Fixing first risked rewriting the file on disk and
                // then failing to re-read it, leaving it altered with no output
                // (#10).
                // `fix_file` now applies every independent safe fix and drops
                // only the genuinely unsafe edits, so an `Err` here means *no*
                // edit could be applied safely — a real failure to honour
                // `--fix`, not a silently-skipped subset. Flag it so the process
                // exits non-zero rather than misleadingly reporting success (#75).
                if args.fix
                    && !args.diff
                    && let Err(e) = runner.fix_file(path)
                {
                    eprintln!("warning: could not fix {}: {}", path.display(), e);
                    any_error = true;
                }
            }
            // A per-file read error (a genuinely unreadable path: missing,
            // permission-denied, a directory) must not abort the whole batch —
            // report it, mark the run failed, and keep linting later files.
            // Deferring the non-zero exit to after the loop (and making it the
            // lint-failure code 1, not the usage/abort code 2) mirrors m1-fmt's
            // per-file loop, so `m1-lint Scripts/*.m1scr` no longer leaves an
            // unknown number of scripts unchecked behind one file (#66).
            Err(e) => {
                eprintln!("error: could not read {}: {}", path.display(), e);
                any_error = true;
                continue;
            }
        }
    }

    match args.format {
        Format::Json => println!("{}", report::render_json(&json_files)),
        Format::Sarif => println!("{}", report::render_sarif(&json_files)),
        Format::Human => {}
    }
    if let (Some(path), Some(nb)) = (&args.write_baseline, &new_baseline) {
        if let Err(e) = m1_workspace::atomic_write(path, nb.to_json().as_bytes()) {
            fail(&format!("could not write baseline {}: {e}", path.display()));
        }
        eprintln!("wrote baseline {}", path.display());
        // A baseline-writing run is an adoption step, not a gate: exit 0 so
        // CI can snapshot without failing on the pre-existing findings.
        process::exit(0);
    }
    if any_error || (args.diff && any_diff) {
        process::exit(1);
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("error: {msg}");
    process::exit(2);
}
fn cfg_fail(e: m1_lint::config::ConfigError) -> ! {
    fail(&e.to_string())
}

/// Split a comma-separated `--select`/`--ignore` value into trimmed, non-empty
/// codes (e.g. `"L001, L004"` → `["L001", "L004"]`).
fn split_codes(v: &str) -> Vec<String> {
    v.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Args, split_codes};
    use clap::Parser;

    fn parse(args: &[&str]) -> Args {
        let mut v = vec!["m1-lint"];
        v.extend_from_slice(args);
        Args::parse_from(v)
    }

    #[test]
    fn splits_comma_codes() {
        assert_eq!(split_codes("L001,L004"), vec!["L001", "L004"]);
        assert_eq!(split_codes("L001, L004 "), vec!["L001", "L004"]);
        assert!(split_codes("").is_empty());
        assert!(split_codes(" , ").is_empty());
    }

    #[test]
    fn accepts_flag_equals_value_form() {
        // clap accepts both `--format=json` and `--format json`; the GNU
        // `=`-form that the old hand-rolled parser had to normalise by hand.
        let a = parse(&["--format=json", "x.m1scr"]);
        assert_eq!(a.format, super::Format::Json);
        let b = parse(&["--select=L010", "x.m1scr"]);
        assert_eq!(b.select.as_deref(), Some("L010"));
    }

    #[test]
    fn accepts_space_separated_form() {
        let a = parse(&["--format", "json", "x.m1scr"]);
        assert_eq!(a.format, super::Format::Json);
    }

    #[test]
    fn defaults_to_human_no_fix() {
        let a = parse(&["x.m1scr"]);
        assert_eq!(a.format, super::Format::Human);
        assert!(!a.fix);
        assert!(!a.rules);
        assert_eq!(a.files.len(), 1);
    }

    #[test]
    fn parses_thresholds_and_files() {
        let a = parse(&[
            "--max-line-length",
            "100",
            "--max-complexity=12",
            "--fix",
            "a.m1scr",
            "b.m1scr",
        ]);
        assert_eq!(a.max_line_length, Some(100));
        assert_eq!(a.max_complexity, Some(12));
        assert!(a.fix);
        assert_eq!(a.files.len(), 2);
    }
}
