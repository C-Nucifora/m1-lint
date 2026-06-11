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
    /// Files or directories to lint (directories are searched recursively for
    /// `.m1scr`; a lone `-`, or no files, reads from stdin)
    files: Vec<PathBuf>,

    /// Filename to use when reading from stdin
    #[arg(long, default_value = "<stdin>")]
    stdin_filename: String,

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

    /// Number of worker threads for multi-file runs (default: one per core;
    /// `--jobs 1` forces serial processing)
    #[arg(long, value_name = "N")]
    jobs: Option<usize>,
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

    // stdin input (#119): a lone `-`, or no files at all, lints stdin —
    // mirroring m1-fmt's CLI surface. A `-` mixed with real paths is ambiguous.
    let use_stdin =
        args.files.is_empty() || (args.files.len() == 1 && args.files[0].as_os_str() == "-");
    if !use_stdin && args.files.iter().any(|f| f.as_os_str() == "-") {
        fail("`-` (stdin) cannot be combined with file paths");
    }
    if use_stdin {
        run_stdin(
            &args,
            &select,
            &ignore,
            baseline.as_ref(),
            new_baseline.as_mut(),
            &mut json_files,
            &mut any_error,
            &mut any_diff,
        );
    }

    // Directory arguments expand to every `.m1scr` beneath them via the shared
    // hardened walk (#125). An empty expansion is reported per-path and treated
    // like a per-file read error: flag the run, keep going, exit 1 — a mistyped
    // path must not look like a clean tree.
    let mut files: Vec<PathBuf> = Vec::new();
    if !use_stdin {
        for f in &args.files {
            if f.is_dir() {
                let found = m1_workspace::find_scripts(f);
                if found.is_empty() {
                    eprintln!("error: no .m1scr files found under {}", f.display());
                    any_error = true;
                } else {
                    files.extend(found);
                }
            } else {
                files.push(f.clone());
            }
        }
    }

    // Resolve config once per unique parent directory, not once per file:
    // `resolve_config` walks the directory tree upward (filesystem I/O) and a
    // Runner/Registry build per file is pure waste when 200 scripts share a
    // directory. The exclude globs are compiled once per entry too (#127).
    let mut contexts: std::collections::HashMap<PathBuf, DirContext> =
        std::collections::HashMap::new();
    for path in &files {
        let dir = m1_lint::config::dir_of(path);
        contexts.entry(dir).or_insert_with_key(|dir| {
            let cfg = resolve_config(&args, &select, &ignore, dir);
            DirContext {
                matcher: m1_lint::config::ExcludeMatcher::new(&cfg),
                runner: Runner::new(Registry::from_config(&cfg)),
            }
        });
    }

    // Lint files in parallel (#131): per-file work is CPU-bound (parse + rule
    // walk) and share-nothing once configs are resolved. Each file produces a
    // FileOutcome; rendering happens serially afterwards in input order, so
    // output (and exit codes) stay byte-deterministic.
    if let Some(n) = args.jobs {
        // Errors only if a global pool already exists (e.g. in tests) — the
        // default pool is then used, which is acceptable.
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global();
    }
    use rayon::prelude::*;
    let outcomes: Vec<Option<FileOutcome>> = files
        .par_iter()
        .map(|path| {
            let ctx = &contexts[&m1_lint::config::dir_of(path)];

            // Skip files matching an `exclude` glob from the config (#9).
            if ctx.matcher.is_excluded(path) {
                return None;
            }
            let display = path.display().to_string();

            // One tolerant read serves linting, baseline anchoring, and --diff.
            // The strict `read_to_string().unwrap_or_default()` re-reads
            // previously used for the latter two silently collapsed non-UTF-8
            // (Windows-1252) sources to "", losing the baseline's content anchor
            // and making --diff disagree with --fix (#124).
            let source = match m1_workspace::read_text(path) {
                Ok(s) => s,
                // A per-file read error (a genuinely unreadable path: missing,
                // permission-denied) must not abort the whole batch — report
                // it, mark the run failed, and keep linting later files (#66).
                Err(e) => {
                    return Some(FileOutcome {
                        display,
                        read_err: Some(e.to_string()),
                        ..Default::default()
                    });
                }
            };

            let mut result = ctx.runner.run_source(&source);
            if let Some(b) = &baseline {
                b.filter(&display, &source, &mut result.diagnostics);
            }
            let mut out = FileOutcome {
                error: !result.syntax_errors.is_empty()
                    || result
                        .diagnostics
                        .iter()
                        .any(|d| d.inner.severity == m1_core::Severity::Error),
                display,
                ..Default::default()
            };

            // --diff (#112): preview what --fix would change; never write.
            if args.diff {
                match ctx.runner.fix_source_stable(&source) {
                    Ok(Some(fixed)) => {
                        out.diff = Some(m1_workspace::diff::unified_diff(
                            &out.display,
                            &source,
                            &fixed,
                        ));
                    }
                    Ok(None) => {}
                    Err(e) => {
                        out.warning = Some(format!(
                            "warning: could not compute fixes for {}: {e}",
                            out.display
                        ));
                        out.error = true;
                    }
                }
            }

            // Apply fixes only after linting completed without an I/O error.
            // Fixing first risked rewriting the file on disk and then failing
            // to re-read it, leaving it altered with no output (#10).
            // `fix_file` applies every independent safe fix and drops only the
            // genuinely unsafe edits, so an `Err` here means *no* edit could be
            // applied safely — a real failure to honour `--fix`, not a
            // silently-skipped subset. Flag it so the process exits non-zero
            // rather than misleadingly reporting success (#75).
            if args.fix && !args.diff {
                match ctx.runner.fix_file(path) {
                    // Count rewrites so --fix can report what it did (#137).
                    Ok(changed) => out.fixed = changed,
                    Err(e) => {
                        out.warning =
                            Some(format!("warning: could not fix {}: {}", out.display, e));
                        out.error = true;
                    }
                }
            }

            out.source = Some(source);
            out.result = Some(result);
            Some(out)
        })
        .collect();

    // Render serially, in input order (deterministic output; the baseline
    // recorder needs `&mut` and human/JSON streams must not interleave).
    let mut files_seen = 0usize;
    let mut files_fixed = 0usize;
    for out in outcomes.into_iter().flatten() {
        let FileOutcome {
            display,
            source,
            read_err,
            result,
            diff,
            warning,
            error,
            fixed,
        } = out;
        if let Some(e) = read_err {
            eprintln!("error: could not read {display}: {e}");
            any_error = true;
            continue;
        }
        files_seen += 1;
        if fixed {
            files_fixed += 1;
        }
        let (Some(source), Some(result)) = (source, result) else {
            continue;
        };
        if let Some(nb) = &mut new_baseline {
            nb.record(&display, &source, &result.diagnostics);
        }
        if error {
            any_error = true;
        }
        match args.format {
            Format::Human => eprint!("{}", report::render_human(&display, &result)),
            Format::Json | Format::Sarif => json_files.push((display, result)),
        }
        if let Some(d) = diff {
            print!("{d}");
            any_diff = true;
        }
        if let Some(w) = warning {
            eprintln!("{w}");
        }
    }

    match args.format {
        Format::Json => println!("{}", report::render_json(&json_files)),
        Format::Sarif => println!("{}", report::render_sarif(&json_files)),
        Format::Human => {}
    }
    // --fix was silent about what it did; "0 files" also tells the user the
    // remaining findings are unfixable (#137). Stderr, like the per-file
    // reports, so machine formats on stdout are unaffected.
    if args.fix && !args.diff {
        eprintln!("applied fixes to {files_fixed} of {files_seen} file(s)");
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

/// Lint stdin (#119): read the whole input through the tolerant workspace
/// decoder, resolve config from `--stdin-filename`'s directory (else the CWD),
/// and report under that name. `--fix` writes the fixed source to stdout
/// (in-place is meaningless), so it cannot be combined with a machine format
/// that also claims stdout.
#[allow(clippy::too_many_arguments)]
fn run_stdin(
    args: &Args,
    select: &Option<Vec<String>>,
    ignore: &Option<Vec<String>>,
    baseline: Option<&m1_lint::baseline::Baseline>,
    new_baseline: Option<&mut m1_lint::baseline::Baseline>,
    json_files: &mut Vec<(String, m1_lint::runner::RunResult)>,
    any_error: &mut bool,
    any_diff: &mut bool,
) {
    if args.fix && !args.diff && args.format != Format::Human {
        fail(
            "--fix on stdin writes the fixed source to stdout; it cannot be combined with --format json/sarif",
        );
    }

    let display = args.stdin_filename.clone();
    let pseudo = PathBuf::from(&args.stdin_filename);
    // Parent of a bare filename is the empty path; discover from the CWD then.
    let dir = match pseudo.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let cfg = resolve_config(args, select, ignore, &dir);
    let runner = Runner::new(Registry::from_config(&cfg));

    // Read stdin as bytes and decode through the same tolerant workspace
    // decoder the file path uses (MoTeC sources may carry Windows-1252 bytes).
    let mut bytes = Vec::new();
    if let Err(e) = std::io::Read::read_to_end(&mut std::io::stdin(), &mut bytes) {
        fail(&format!("{display}: {e}"));
    }
    let source = m1_workspace::decode(bytes);

    let mut result = runner.run_source(&source);
    if let Some(b) = baseline {
        b.filter(&display, &source, &mut result.diagnostics);
    }
    if let Some(nb) = new_baseline {
        nb.record(&display, &source, &result.diagnostics);
    }
    if !result.syntax_errors.is_empty()
        || result
            .diagnostics
            .iter()
            .any(|d| d.inner.severity == m1_core::Severity::Error)
    {
        *any_error = true;
    }
    match args.format {
        Format::Human => eprint!("{}", report::render_human(&display, &result)),
        Format::Json | Format::Sarif => json_files.push((display.clone(), result)),
    }

    if args.diff {
        match runner.fix_source_stable(&source) {
            Ok(Some(fixed)) => {
                print!(
                    "{}",
                    m1_workspace::diff::unified_diff(&display, &source, &fixed)
                );
                *any_diff = true;
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!("warning: could not compute fixes for {display}: {e}");
                *any_error = true;
            }
        }
    } else if args.fix {
        match runner.fix_source_stable(&source) {
            Ok(Some(fixed)) => print!("{fixed}"),
            Ok(None) => print!("{source}"),
            Err(e) => {
                // Pass the input through so a pipeline never loses data.
                print!("{source}");
                eprintln!("warning: could not fix {display}: {e}");
                *any_error = true;
            }
        }
    }
}

/// Everything a worker needs to lint files in one directory: the resolved
/// config's compiled exclude globs and the rule runner built from it (#127).
struct DirContext {
    matcher: m1_lint::config::ExcludeMatcher,
    runner: Runner,
}

/// One file's results from the parallel lint phase, rendered serially in input
/// order afterwards (#131). `None` fields mean "not applicable" (e.g. no
/// `--diff` requested, or the read failed before linting).
#[derive(Default)]
struct FileOutcome {
    display: String,
    /// The decoded source (needed later for `--write-baseline` recording).
    source: Option<String>,
    read_err: Option<String>,
    result: Option<m1_lint::runner::RunResult>,
    /// Unified diff for `--diff`.
    diff: Option<String>,
    /// A deferred fix/diff warning line for stderr.
    warning: Option<String>,
    /// Whether this file makes the run exit non-zero.
    error: bool,
    /// Whether `--fix` rewrote this file (`fix_file` returned `Ok(true)`).
    fixed: bool,
}

/// Resolve the effective [`Config`] for a lint target, lowest layer first: the
/// unified m1-tools.toml, then the tool-specific file (explicit `--config`,
/// else discovered `.m1lint.toml` / user-global), then CLI flags. So a project
/// can be configured entirely from m1-tools.toml; a .m1lint.toml still works
/// and overrides it.
fn resolve_config(
    args: &Args,
    select: &Option<Vec<String>>,
    ignore: &Option<Vec<String>>,
    dir: &std::path::Path,
) -> Config {
    let mut cfg = Config::default();
    if let Some(tc) = m1_workspace::config::M1ToolsConfig::discover(dir)
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
            if let Err(e) = cfg.apply_discovered_file(dir) {
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
    cfg
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
