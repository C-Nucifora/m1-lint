//! m1-lint — command-line linter for the MoTeC M1 script language.

use std::path::PathBuf;
use std::process;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("m1-lint {}", env!("CARGO_PKG_VERSION"));
        process::exit(0);
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("usage: m1-lint [--version] [--help] <file>...");
        println!();
        println!("Lint .m1scr files for style and correctness.");
        process::exit(0);
    }
    if args.len() < 2 {
        eprintln!("usage: m1-lint <file>...");
        process::exit(2);
    }
    let files: Vec<PathBuf> = args[1..].iter().map(PathBuf::from).collect();
    let registry = m1_lint::registry::Registry::default_v1();
    let runner = m1_lint::runner::Runner::new(registry);

    let mut any_error = false;
    let mut total_errors = 0usize;
    let mut total_warnings = 0usize;

    for path in &files {
        match runner.run_file(path) {
            Ok(result) => {
                for diag in &result.syntax_errors {
                    eprintln!(
                        "{}:{}:{}: error[syntax]: {}",
                        path.display(),
                        diag.range.start.line + 1,
                        diag.range.start.column + 1,
                        diag.message
                    );
                    total_errors += 1;
                    any_error = true;
                }
                for diag in &result.diagnostics {
                    let sev = match diag.inner.severity {
                        m1_core::Severity::Error => {
                            total_errors += 1;
                            any_error = true;
                            "error"
                        }
                        m1_core::Severity::Warning => {
                            total_warnings += 1;
                            "warning"
                        }
                        m1_core::Severity::Info => "info",
                        m1_core::Severity::Hint => "hint",
                    };
                    eprintln!(
                        "{}:{}:{}: {}[{}]: {}",
                        path.display(),
                        diag.inner.range.start.line + 1,
                        diag.inner.range.start.column + 1,
                        sev,
                        diag.code,
                        diag.inner.message
                    );
                }
            }
            Err(e) => {
                eprintln!("error: could not read {}: {}", path.display(), e);
                process::exit(2);
            }
        }
    }

    eprintln!("{} errors, {} warnings", total_errors, total_warnings);
    if any_error {
        process::exit(1);
    }
}
