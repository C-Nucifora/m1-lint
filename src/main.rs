//! m1-lint — command-line linter for the MoTeC M1 script language.

use std::path::PathBuf;
use std::process;

use m1_lint::config::Config;
use m1_lint::registry::Registry;
use m1_lint::report;
use m1_lint::runner::Runner;

enum Format {
    Human,
    Json,
}

fn main() {
    let raw: Vec<String> = std::env::args().collect();
    // Normalise `--flag=value` into separate `--flag` / `value` tokens so both
    // the GNU `--flag=value` and the space-separated `--flag value` forms work,
    // matching m1-fmt/m1-typecheck (clap). A bare `value` (a file path) that
    // happens to contain `=` is left untouched — only `--`-prefixed tokens split.
    // `--` (end-of-options) and long flags with no value are passed through. (#69)
    let args = normalize_args(&raw[1..]);

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("m1-lint {}", env!("CARGO_PKG_VERSION"));
        process::exit(0);
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        process::exit(0);
    }
    // --rules prints the rule catalogue (the single source of truth for tools
    // that enumerate rules) and exits, honouring --format json|human.
    if args.iter().any(|a| a == "--rules") {
        let json = args
            .windows(2)
            .any(|w| w[0] == "--format" && w[1] == "json");
        if json {
            println!("{}", report::render_rules_json());
        } else {
            print!("{}", report::render_rules_human());
        }
        process::exit(0);
    }

    let mut format = Format::Human;
    let mut do_fix = false;
    let mut config_path: Option<PathBuf> = None;
    let mut max_line: Option<usize> = None;
    let mut max_depth: Option<usize> = None;
    let mut max_complexity: Option<u32> = None;
    let mut max_cognitive_complexity: Option<u32> = None;
    let mut select: Option<Vec<String>> = None;
    let mut ignore: Option<Vec<String>> = None;
    let mut files: Vec<PathBuf> = Vec::new();

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--fix" => do_fix = true,
            "--format" => match it.next().map(String::as_str) {
                Some("human") => format = Format::Human,
                Some("json") => format = Format::Json,
                other => fail(&format!("--format expects human|json, got {other:?}")),
            },
            "--config" => config_path = Some(PathBuf::from(req(it.next(), "--config"))),
            "--max-line-length" => max_line = Some(parse_num(it.next(), "--max-line-length")),
            "--max-nesting-depth" => max_depth = Some(parse_num(it.next(), "--max-nesting-depth")),
            "--max-complexity" => max_complexity = Some(parse_num(it.next(), "--max-complexity")),
            "--max-cognitive-complexity" => {
                max_cognitive_complexity = Some(parse_num(it.next(), "--max-cognitive-complexity"))
            }
            "--select" => select = Some(split_codes(it.next(), "--select")),
            "--ignore" => ignore = Some(split_codes(it.next(), "--ignore")),
            s if s.starts_with("--") => fail(&format!("unknown flag: {s}")),
            s => files.push(PathBuf::from(s)),
        }
    }
    if files.is_empty() {
        fail("no input files");
    }

    let mut any_error = false;
    let mut json_files: Vec<(String, m1_lint::runner::RunResult)> = Vec::new();

    for path in &files {
        // Resolve config: explicit --config, else discover from the file's dir.
        let mut cfg = match &config_path {
            Some(p) => read_config(p),
            None => match Config::discover(&m1_lint::config::dir_of(path)) {
                Ok(c) => c,
                Err(e) => cfg_fail(e),
            },
        };
        if let Some(n) = max_line {
            cfg.max_line_length = n;
        }
        if let Some(n) = max_depth {
            cfg.max_nesting_depth = n;
        }
        if let Some(n) = max_complexity {
            cfg.max_complexity = n;
        }
        if let Some(n) = max_cognitive_complexity {
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
            Ok(result) => {
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
                match format {
                    Format::Human => {
                        eprint!(
                            "{}",
                            report::render_human(&path.display().to_string(), &result)
                        );
                    }
                    Format::Json => json_files.push((path.display().to_string(), result)),
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
                if do_fix && let Err(e) = runner.fix_file(path) {
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

    if let Format::Json = format {
        println!("{}", report::render_json(&json_files));
    }
    if any_error {
        process::exit(1);
    }
}

/// Split any `--flag=value` token into `--flag` and `value`, on the first `=`.
/// Tokens that don't start with `--`, the bare `--` end-of-options marker, and
/// `--flag` tokens without `=` pass through unchanged. This lets the hand-rolled
/// parser accept the GNU `--flag=value` form like clap does (#69).
fn normalize_args(args: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    for arg in args {
        if arg.starts_with("--")
            && arg != "--"
            && let Some(eq) = arg.find('=')
        {
            out.push(arg[..eq].to_string());
            out.push(arg[eq + 1..].to_string());
        } else {
            out.push(arg.clone());
        }
    }
    out
}

fn fail(msg: &str) -> ! {
    eprintln!("error: {msg}");
    process::exit(2);
}
fn cfg_fail(e: m1_lint::config::ConfigError) -> ! {
    fail(&e.to_string())
}
fn req<'a>(v: Option<&'a String>, flag: &str) -> &'a str {
    v.map(String::as_str)
        .unwrap_or_else(|| fail(&format!("{flag} requires a value")))
}
fn parse_num<T: std::str::FromStr>(v: Option<&String>, flag: &str) -> T {
    req(v, flag)
        .parse()
        .unwrap_or_else(|_| fail(&format!("{flag} expects a number")))
}
fn split_codes(v: Option<&String>, flag: &str) -> Vec<String> {
    req(v, flag)
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}
fn read_config(p: &std::path::Path) -> Config {
    let text = match std::fs::read_to_string(p) {
        Ok(t) => t,
        Err(e) => fail(&format!("could not read {}: {}", p.display(), e)),
    };
    match Config::from_toml_str(&text) {
        Ok(c) => c,
        Err(e) => cfg_fail(e),
    }
}
fn print_help() {
    println!("usage: m1-lint [OPTIONS] <file>...");
    println!();
    println!("OPTIONS:");
    println!("  --format <human|json>    output format (default: human)");
    println!("  --fix                    apply safe autofixes in place");
    println!("  --config <path>          use this .m1lint.toml");
    println!("  --max-line-length <N>");
    println!("  --max-nesting-depth <N>");
    println!("  --max-complexity <N>             cyclomatic complexity ceiling (L009)");
    println!("  --max-cognitive-complexity <N>   cognitive complexity ceiling (L019)");
    println!("  --select <CODES>         comma-separated; only these rules run");
    println!("  --ignore <CODES>         comma-separated; remove these rules");
    println!("  --rules                  print the rule catalogue (with --format json) and exit");
    println!("  -h, --help");
    println!("  -V, --version");
    println!();
    println!("--fix makes minimal edits; for full canonical formatting use m1-fmt.");
}

#[cfg(test)]
mod tests {
    use super::normalize_args;

    fn norm(args: &[&str]) -> Vec<String> {
        normalize_args(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn splits_flag_equals_value() {
        assert_eq!(norm(&["--format=json"]), vec!["--format", "json"]);
        assert_eq!(norm(&["--select=L010"]), vec!["--select", "L010"]);
    }

    #[test]
    fn splits_on_first_equals_only() {
        // A value may itself contain `=` (e.g. a path); only the first `=` splits.
        assert_eq!(norm(&["--config=a=b.toml"]), vec!["--config", "a=b.toml"]);
    }

    #[test]
    fn passes_through_space_form_and_bare_tokens() {
        assert_eq!(norm(&["--format", "json"]), vec!["--format", "json"]);
        assert_eq!(norm(&["--fix"]), vec!["--fix"]);
        // A bare positional containing `=` (not `--`-prefixed) is untouched.
        assert_eq!(norm(&["a=b.m1scr"]), vec!["a=b.m1scr"]);
        // The end-of-options marker is preserved verbatim.
        assert_eq!(norm(&["--"]), vec!["--"]);
    }
}
