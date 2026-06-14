//! Effective lint configuration: thresholds + the active rule set.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::diagnostic::LintCode;

/// Which character indentation must use (L010). The M1 manual mandates tabs, so
/// that is the default; teams preferring spaces set `indent-style = "spaces"`.
///
/// Re-exported from `m1-workspace`: the formatter and the linter share one
/// definition (same variants, same default of `Tab`, same string spellings)
/// instead of each carrying a copy.
pub use m1_workspace::IndentStyle;

/// Which brace placement the manual mandates (L028). The M1 manual mandates
/// Allman ("a separate line for each brace"), so that is the default; teams
/// preferring K&R set `brace-style = "kr"`.
///
/// Re-exported from `m1-workspace`: the formatter and the linter share one
/// definition (same variants, same `Allman` default, same string spellings).
pub use m1_workspace::BraceStyle;

/// The resolved configuration the runner uses.
#[derive(Debug, Clone)]
pub struct Config {
    pub max_line_length: usize,
    pub max_nesting_depth: usize,
    pub max_complexity: u32,
    pub max_cognitive_complexity: u32,
    /// Required indentation character (L010). Defaults to tabs, per the manual.
    pub indent_style: IndentStyle,
    /// Required brace placement (L028). Defaults to Allman, per the manual.
    pub brace_style: BraceStyle,
    pub enabled: BTreeSet<LintCode>,
    /// Glob patterns; a file whose path or name matches any is skipped (#9).
    pub exclude: Vec<String>,
    /// Per-rule severity overrides (`[severity]` table: `L001 = "error"`),
    /// applied to each diagnostic after its rule runs (#110).
    pub severity_overrides: std::collections::BTreeMap<LintCode, m1_core::Severity>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            // 88 is a tool convention (shared with m1-fmt), not an M1 manual
            // rule — the manual sets no numeric line limit. See L001's docs.
            max_line_length: 88,
            max_nesting_depth: 4,
            max_complexity: 40,
            max_cognitive_complexity: 15,
            indent_style: IndentStyle::default(),
            brace_style: BraceStyle::default(),
            severity_overrides: std::collections::BTreeMap::new(),
            enabled: LintCode::all_codes()
                .iter()
                .copied()
                .filter(|c| !c.off_by_default())
                .collect(),
            exclude: Vec::new(),
        }
    }
}

/// Raw, fully-optional view parsed from `.m1lint.toml`.
#[derive(Debug, Default)]
struct RawConfig {
    max_line_length: Option<usize>,
    max_nesting_depth: Option<usize>,
    max_complexity: Option<u32>,
    max_cognitive_complexity: Option<u32>,
    indent_style: Option<IndentStyle>,
    brace_style: Option<BraceStyle>,
    select: Option<Vec<String>>,
    ignore: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
    severity: Option<Vec<(String, String)>>,
}

/// A configuration error (maps to CLI exit code 2).
#[derive(Debug)]
pub enum ConfigError {
    Toml(String),
    UnknownKey(String),
    UnknownCode(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Toml(e) => write!(f, "invalid .m1lint.toml: {e}"),
            ConfigError::UnknownKey(k) => write!(f, "unknown config key: {k}"),
            ConfigError::UnknownCode(c) => write!(f, "unknown lint code: {c}"),
        }
    }
}

impl Config {
    /// Parse a `.m1lint.toml` string into a raw view, then merge over defaults.
    pub fn from_toml_str(s: &str) -> Result<Config, ConfigError> {
        let raw = parse_raw(s)?;
        let mut cfg = Config::default();
        cfg.apply_raw(raw)?;
        Ok(cfg)
    }

    /// Walk up from `start_dir` looking for a `.m1lint.toml`. Returns the
    /// parsed config if found, else `Config::default()`.
    ///
    /// The resetting counterpart of [`Self::apply_discovered_file`]: because it
    /// always starts from `Config::default()`, it is exactly equivalent to
    /// applying the discovered file onto a fresh default, so it delegates there
    /// rather than carrying its own copy of the upward-walk + global fallback.
    pub fn discover(start_dir: &Path) -> Result<Config, ConfigError> {
        let mut cfg = Config::default();
        cfg.apply_discovered_file(start_dir)?;
        Ok(cfg)
    }

    fn apply_raw(&mut self, raw: RawConfig) -> Result<(), ConfigError> {
        if let Some(n) = raw.max_line_length {
            self.max_line_length = n;
        }
        if let Some(n) = raw.max_nesting_depth {
            self.max_nesting_depth = n;
        }
        if let Some(n) = raw.max_complexity {
            self.max_complexity = n;
        }
        if let Some(n) = raw.max_cognitive_complexity {
            self.max_cognitive_complexity = n;
        }
        if let Some(style) = raw.indent_style {
            self.indent_style = style;
        }
        if let Some(style) = raw.brace_style {
            self.brace_style = style;
        }
        if let Some(ex) = raw.exclude {
            self.exclude = ex;
        }
        if let Some(sev) = raw.severity {
            for (code_str, level_str) in sev {
                let code =
                    LintCode::from_code_str(&code_str).ok_or(ConfigError::UnknownCode(code_str))?;
                let level = parse_severity(&level_str)?;
                self.severity_overrides.insert(code, level);
            }
        }
        self.apply_filters(raw.select, raw.ignore)
    }

    /// True if `path` matches any configured `exclude` glob, tested against both
    /// the full path and the bare file name (so `*.gen.m1scr` and
    /// `generated/*` both work).
    ///
    /// Compiles the globs on every call. A caller matching many paths against
    /// one config (the CLI's file loop) should build an [`ExcludeMatcher`] once
    /// instead (#127).
    pub fn is_excluded(&self, path: &Path) -> bool {
        ExcludeMatcher::new(self).is_excluded(path)
    }

    /// Apply select-then-ignore over the current `enabled` set.
    pub fn apply_filters(
        &mut self,
        select: Option<Vec<String>>,
        ignore: Option<Vec<String>>,
    ) -> Result<(), ConfigError> {
        if let Some(sel) = select {
            let mut set = BTreeSet::new();
            for s in sel {
                let code = LintCode::from_code_str(&s).ok_or(ConfigError::UnknownCode(s))?;
                set.insert(code);
            }
            self.enabled = set;
        }
        if let Some(ign) = ignore {
            for s in ign {
                let code = LintCode::from_code_str(&s).ok_or(ConfigError::UnknownCode(s))?;
                self.enabled.remove(&code);
            }
        }
        Ok(())
    }

    /// Overlay the unified `m1-tools.toml` onto this config (only set fields).
    /// Reads `[lint]` thresholds + `exclude`, the shared `[format].indent_style`
    /// (the indent character is one decision shared with the formatter), and
    /// `[diagnostics]` select/ignore.
    pub fn apply_tools_config(
        &mut self,
        tc: &m1_workspace::config::M1ToolsConfig,
    ) -> Result<(), ConfigError> {
        if let Some(n) = tc.lint.max_line_length {
            self.max_line_length = n;
        }
        if let Some(n) = tc.lint.max_nesting_depth {
            self.max_nesting_depth = n;
        }
        if let Some(n) = tc.lint.max_complexity {
            self.max_complexity = n;
        }
        if let Some(n) = tc.lint.max_cognitive_complexity {
            self.max_cognitive_complexity = n;
        }
        if let Some(ex) = &tc.lint.exclude {
            self.exclude = ex.clone();
        }
        if let Some(s) = tc.format.indent_style.as_deref() {
            self.indent_style = IndentStyle::parse(s)
                .ok_or_else(|| ConfigError::Toml(format!("invalid indent_style: {s}")))?;
        }
        if let Some(s) = tc.format.brace_style.as_deref() {
            self.brace_style = BraceStyle::parse(s)
                .ok_or_else(|| ConfigError::Toml(format!("invalid brace_style: {s}")))?;
        }
        self.apply_filters(tc.diagnostics.select.clone(), tc.diagnostics.ignore.clone())
    }

    /// Overlay a `.m1lint.toml` body (raw, only set fields) onto this config.
    pub fn apply_toml_str(&mut self, s: &str) -> Result<(), ConfigError> {
        let raw = parse_raw(s)?;
        self.apply_raw(raw)
    }

    /// If a `.m1lint.toml` (walking up from `start_dir`) or the user-global config
    /// exists, overlay it (only its set keys) onto this config; return whether one
    /// was found. The non-resetting counterpart of [`Self::discover`].
    ///
    /// This is the single owner of the upward-walk + global-config fallback;
    /// [`Self::discover`] delegates here against a fresh `Config::default()`.
    pub fn apply_discovered_file(&mut self, start_dir: &Path) -> Result<bool, ConfigError> {
        let mut dir: Option<&Path> = Some(start_dir);
        while let Some(d) = dir {
            let cand = d.join(".m1lint.toml");
            if cand.is_file() {
                let text =
                    std::fs::read_to_string(&cand).map_err(|e| ConfigError::Toml(e.to_string()))?;
                self.apply_toml_str(&text)?;
                return Ok(true);
            }
            dir = d.parent();
        }
        if let Some(global) = global_config_path()
            && global.is_file()
        {
            let text =
                std::fs::read_to_string(&global).map_err(|e| ConfigError::Toml(e.to_string()))?;
            self.apply_toml_str(&text)?;
            return Ok(true);
        }
        Ok(false)
    }
}

/// The `exclude` globs of one [`Config`], pre-compiled. `glob::Pattern::new`
/// re-parses the pattern text on every call, so testing N files against K
/// patterns through [`Config::is_excluded`] costs K×N compilations; compiling
/// once here makes it K (#127). Invalid patterns are skipped, matching
/// `Config::is_excluded`'s historical behaviour.
#[derive(Debug, Clone)]
pub struct ExcludeMatcher {
    patterns: Vec<glob::Pattern>,
}

impl ExcludeMatcher {
    pub fn new(cfg: &Config) -> Self {
        ExcludeMatcher {
            patterns: cfg
                .exclude
                .iter()
                .filter_map(|pat| glob::Pattern::new(pat).ok())
                .collect(),
        }
    }

    /// True if `path` matches any compiled glob, tested against both the full
    /// path and the bare file name (so `*.gen.m1scr` and `generated/*` work).
    pub fn is_excluded(&self, path: &Path) -> bool {
        if self.patterns.is_empty() {
            return false;
        }
        let full = path.to_string_lossy();
        let name = path.file_name().map(|n| n.to_string_lossy());
        self.patterns
            .iter()
            .any(|p| p.matches(&full) || name.as_deref().is_some_and(|n| p.matches(n)))
    }
}

fn parse_raw(s: &str) -> Result<RawConfig, ConfigError> {
    // Parse the document as a TOML table. (toml 1.x changed `str::parse::<Value>`
    // to expect a bare value, not a document, so a `key = val` config failed to
    // parse — parse a `Table` directly instead.)
    let table: toml::Table = s
        .parse()
        .map_err(|e: toml::de::Error| ConfigError::Toml(e.to_string()))?;

    let mut raw = RawConfig::default();
    for (k, v) in &table {
        // Accept both the kebab-case `.m1lint.toml` dialect and the snake_case
        // keys used by the unified `m1-tools.toml` / `--scaffold-config` output,
        // so the scaffold can be used directly as a `.m1lint.toml` (#84).
        match k.replace('_', "-").as_str() {
            "max-line-length" => raw.max_line_length = v.as_integer().map(|n| n as usize),
            "max-nesting-depth" => raw.max_nesting_depth = v.as_integer().map(|n| n as usize),
            "max-complexity" => raw.max_complexity = v.as_integer().map(|n| n as u32),
            "max-cognitive-complexity" => {
                raw.max_cognitive_complexity = v.as_integer().map(|n| n as u32)
            }
            "indent-style" => {
                let s = v
                    .as_str()
                    .ok_or_else(|| ConfigError::Toml("indent-style must be a string".into()))?;
                raw.indent_style = Some(
                    IndentStyle::parse(s)
                        .ok_or_else(|| ConfigError::Toml(format!("invalid indent-style: {s}")))?,
                );
            }
            "brace-style" => {
                let s = v
                    .as_str()
                    .ok_or_else(|| ConfigError::Toml("brace-style must be a string".into()))?;
                raw.brace_style = Some(
                    BraceStyle::parse(s)
                        .ok_or_else(|| ConfigError::Toml(format!("invalid brace-style: {s}")))?,
                );
            }
            "select" => raw.select = Some(string_array(v)?),
            "severity" => {
                let table = v.as_table().ok_or_else(|| {
                    ConfigError::Toml("severity must be a table of CODE = \"level\"".into())
                })?;
                let mut pairs = Vec::new();
                for (code, level) in table {
                    let level = level.as_str().ok_or_else(|| {
                        ConfigError::Toml(format!("severity.{code} must be a string"))
                    })?;
                    pairs.push((code.clone(), level.to_string()));
                }
                raw.severity = Some(pairs);
            }
            "ignore" => raw.ignore = Some(string_array(v)?),
            "exclude" => raw.exclude = Some(string_array(v)?),
            // Report the key as the user wrote it.
            _ => return Err(ConfigError::UnknownKey(k.to_string())),
        }
    }
    Ok(raw)
}

/// Parse a severity level name for the `[severity]` override table.
fn parse_severity(s: &str) -> Result<m1_core::Severity, ConfigError> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "error" => m1_core::Severity::Error,
        "warning" | "warn" => m1_core::Severity::Warning,
        "info" => m1_core::Severity::Info,
        "hint" => m1_core::Severity::Hint,
        _ => {
            return Err(ConfigError::Toml(format!(
                "invalid severity level {s:?}; expected error|warning|info|hint"
            )));
        }
    })
}

fn string_array(v: &toml::Value) -> Result<Vec<String>, ConfigError> {
    v.as_array()
        .ok_or_else(|| ConfigError::Toml("expected array of strings".into()))?
        .iter()
        .map(|e| {
            e.as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| ConfigError::Toml("expected string in array".into()))
        })
        .collect()
}

/// The user-global config path: `$XDG_CONFIG_HOME/m1lint/config.toml`, or
/// `$HOME/.config/m1lint/config.toml`. `None` if neither env var is set.
fn global_config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("m1lint").join("config.toml"))
}

/// Helper for callers (CLI/tests) needing the config's directory base.
pub fn dir_of(path: &Path) -> PathBuf {
    path.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indent_style_defaults_to_tab() {
        assert_eq!(Config::default().indent_style, IndentStyle::Tab);
    }

    #[test]
    fn indent_style_parsed_from_toml() {
        let c = Config::from_toml_str("indent-style = \"spaces\"\n").unwrap();
        assert_eq!(c.indent_style, IndentStyle::Spaces);
    }

    #[test]
    fn invalid_indent_style_is_an_error() {
        assert!(Config::from_toml_str("indent-style = \"tabz\"\n").is_err());
    }

    #[test]
    fn accepts_snake_case_keys_from_the_scaffold() {
        // The unified m1-tools.toml / `m1-lsp --scaffold-config` uses snake_case;
        // a user pointing m1-lint at such a file (or copying it to .m1lint.toml)
        // must not be rejected with "unknown config key" (#84).
        let c = Config::from_toml_str(
            "max_line_length = 100\nmax_cognitive_complexity = 9\nindent_style = \"spaces\"\n",
        )
        .unwrap();
        assert_eq!(c.max_line_length, 100);
        assert_eq!(c.indent_style, IndentStyle::Spaces);
    }

    #[test]
    fn genuinely_unknown_key_still_errors() {
        assert!(Config::from_toml_str("not_a_real_key = 1\n").is_err());
    }

    #[test]
    fn default_enables_all_on_by_default_rules() {
        // Derived from the catalogue so a new rule can't make this stale:
        // every code is enabled except the off-by-default ones (L017, L027).
        let off = LintCode::all_codes()
            .iter()
            .filter(|c| c.off_by_default())
            .count();
        assert_eq!(
            Config::default().enabled.len(),
            LintCode::all_codes().len() - off
        );
        assert!(!Config::default().enabled.contains(&LintCode::L017));
        assert!(!Config::default().enabled.contains(&LintCode::L027));
        assert_eq!(Config::default().max_line_length, 88);
    }

    #[test]
    fn parses_thresholds() {
        let cfg = Config::from_toml_str("max-line-length = 100\nmax-complexity = 12\n").unwrap();
        assert_eq!(cfg.max_line_length, 100);
        assert_eq!(cfg.max_complexity, 12);
        assert_eq!(cfg.max_nesting_depth, 4); // untouched default
    }

    #[test]
    fn parses_cognitive_complexity_threshold() {
        let cfg = Config::from_toml_str("max-cognitive-complexity = 20\n").unwrap();
        assert_eq!(cfg.max_cognitive_complexity, 20);
        assert_eq!(cfg.max_complexity, 40); // L009 default untouched
    }

    #[test]
    fn loosened_cyclomatic_default() {
        // L009 default loosened (was 10) now that L019 cognitive is the primary
        // complexity gate; L009 only catches pathological cyclomatic.
        assert_eq!(Config::default().max_complexity, 40);
        assert_eq!(Config::default().max_cognitive_complexity, 15);
    }

    #[test]
    fn select_restricts() {
        let cfg = Config::from_toml_str("select = [\"L001\", \"L004\"]\n").unwrap();
        assert_eq!(cfg.enabled.len(), 2);
        assert!(cfg.enabled.contains(&LintCode::L001));
        assert!(!cfg.enabled.contains(&LintCode::L006));
    }

    #[test]
    fn ignore_subtracts() {
        let cfg = Config::from_toml_str("ignore = [\"L007\"]\n").unwrap();
        assert!(!cfg.enabled.contains(&LintCode::L007));
        assert!(cfg.enabled.contains(&LintCode::L001));
    }

    #[test]
    fn unknown_key_errors() {
        assert!(matches!(
            Config::from_toml_str("max-lien-length = 100\n"),
            Err(ConfigError::UnknownKey(_))
        ));
    }

    #[test]
    fn unknown_code_errors() {
        assert!(matches!(
            Config::from_toml_str("select = [\"L999\"]\n"),
            Err(ConfigError::UnknownCode(_))
        ));
    }

    #[test]
    fn discover_walks_up_to_default_when_absent() {
        let tmp = std::env::temp_dir();
        // A directory unlikely to contain .m1lint.toml up its chain in CI.
        // Compare against the catalogue-derived default, not a hand count
        // that goes stale when a rule is added (#133's stale-count lesson).
        let cfg = Config::discover(&tmp).unwrap();
        assert!(cfg.enabled.len() <= Config::default().enabled.len());
    }

    #[test]
    fn discover_equals_default_plus_apply_discovered_file() {
        // `discover` and `apply_discovered_file` walk the same directory tree
        // for `.m1lint.toml` and share the same global fallback; the only
        // difference is `discover` starts fresh from `Config::default()`.
        // So discovering must equal applying the discovered file onto a
        // default config — guarding the shared walk against drift when one
        // copy is changed but not the other.
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(
            tmp.path().join(".m1lint.toml"),
            "max-line-length = 120\nignore = [\"L007\"]\n",
        )
        .unwrap();

        let discovered = Config::discover(&nested).unwrap();

        let mut overlaid = Config::default();
        let found = overlaid.apply_discovered_file(&nested).unwrap();
        assert!(found, "the walk should locate the planted .m1lint.toml");

        // Parity on the fields the file actually set, and on the rule set.
        assert_eq!(discovered.max_line_length, overlaid.max_line_length);
        assert_eq!(discovered.max_line_length, 120);
        assert_eq!(discovered.enabled, overlaid.enabled);
        assert!(!discovered.enabled.contains(&LintCode::L007));
    }

    #[test]
    fn parses_and_applies_exclude_globs() {
        let cfg = Config::from_toml_str("exclude = [\"*.gen.m1scr\", \"generated/*\"]\n").unwrap();
        assert!(cfg.is_excluded(Path::new("foo.gen.m1scr")));
        assert!(
            cfg.is_excluded(Path::new("a/b/foo.gen.m1scr")),
            "matches on the bare file name too"
        );
        assert!(cfg.is_excluded(Path::new("generated/x.m1scr")));
        assert!(!cfg.is_excluded(Path::new("src/real.m1scr")));
    }

    #[test]
    fn no_exclude_skips_nothing() {
        assert!(!Config::default().is_excluded(Path::new("anything.m1scr")));
    }

    #[test]
    fn exclude_matcher_agrees_with_config_is_excluded() {
        let cfg = Config::from_toml_str("exclude = [\"*.gen.m1scr\", \"generated/*\"]\n").unwrap();
        let m = ExcludeMatcher::new(&cfg);
        for p in [
            "foo.gen.m1scr",
            "a/b/foo.gen.m1scr",
            "generated/x.m1scr",
            "src/real.m1scr",
        ] {
            assert_eq!(m.is_excluded(Path::new(p)), cfg.is_excluded(Path::new(p)));
        }
        // An invalid pattern is skipped, not fatal — same as is_excluded.
        let bad = Config::from_toml_str("exclude = [\"[\", \"*.gen.m1scr\"]\n").unwrap();
        assert!(ExcludeMatcher::new(&bad).is_excluded(Path::new("foo.gen.m1scr")));
        assert!(!ExcludeMatcher::new(&bad).is_excluded(Path::new("real.m1scr")));
    }

    #[test]
    fn unified_then_tool_file_then_flags() {
        let tc = m1_workspace::config::M1ToolsConfig::from_toml_str(
            "[lint]\nmax_line_length = 100\nmax_complexity = 12\nmax_cognitive_complexity = 9\n\
             [format]\nindent_style = \"spaces\"\n",
        )
        .unwrap();
        let mut cfg = Config::default();
        cfg.apply_tools_config(&tc).unwrap();
        assert_eq!(cfg.max_line_length, 100);
        assert_eq!(cfg.max_complexity, 12);
        assert_eq!(cfg.max_cognitive_complexity, 9);
        assert_eq!(cfg.indent_style, IndentStyle::Spaces);
        // Tool file overrides one key; the rest of the unified values survive.
        cfg.apply_toml_str("max-line-length = 120\n").unwrap();
        assert_eq!(cfg.max_line_length, 120);
        assert_eq!(cfg.max_complexity, 12, "unified value survives");
    }

    #[test]
    fn unified_diagnostics_filter_applies() {
        let tc = m1_workspace::config::M1ToolsConfig::from_toml_str(
            "[diagnostics]\nignore = [\"L007\"]\n",
        )
        .unwrap();
        let mut cfg = Config::default();
        cfg.apply_tools_config(&tc).unwrap();
        assert!(!cfg.enabled.contains(&LintCode::L007));
        assert!(cfg.enabled.contains(&LintCode::L001));
    }
}
