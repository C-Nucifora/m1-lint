//! Effective lint configuration: thresholds + the active rule set.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::diagnostic::LintCode;

/// The resolved configuration the runner uses.
#[derive(Debug, Clone)]
pub struct Config {
    pub max_line_length: usize,
    pub max_nesting_depth: usize,
    pub max_complexity: u32,
    pub enabled: BTreeSet<LintCode>,
    /// Glob patterns; a file whose path or name matches any is skipped (#9).
    pub exclude: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            max_line_length: 88,
            max_nesting_depth: 4,
            max_complexity: 10,
            enabled: LintCode::all_codes().iter().copied().collect(),
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
    select: Option<Vec<String>>,
    ignore: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
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
    pub fn discover(start_dir: &Path) -> Result<Config, ConfigError> {
        let mut dir: Option<&Path> = Some(start_dir);
        while let Some(d) = dir {
            let candidate = d.join(".m1lint.toml");
            if candidate.is_file() {
                let text = std::fs::read_to_string(&candidate)
                    .map_err(|e| ConfigError::Toml(e.to_string()))?;
                return Config::from_toml_str(&text);
            }
            dir = d.parent();
        }
        // No project `.m1lint.toml`: fall back to the user-global config
        // (`$XDG_CONFIG_HOME/m1lint/config.toml`, else `~/.config/...`) if present (#9).
        if let Some(global) = global_config_path()
            && global.is_file()
        {
            let text =
                std::fs::read_to_string(&global).map_err(|e| ConfigError::Toml(e.to_string()))?;
            return Config::from_toml_str(&text);
        }
        Ok(Config::default())
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
        if let Some(ex) = raw.exclude {
            self.exclude = ex;
        }
        self.apply_filters(raw.select, raw.ignore)
    }

    /// True if `path` matches any configured `exclude` glob, tested against both
    /// the full path and the bare file name (so `*.gen.m1scr` and
    /// `generated/*` both work).
    pub fn is_excluded(&self, path: &Path) -> bool {
        if self.exclude.is_empty() {
            return false;
        }
        let full = path.to_string_lossy();
        let name = path.file_name().map(|n| n.to_string_lossy());
        self.exclude
            .iter()
            .any(|pat| match glob::Pattern::new(pat) {
                Ok(p) => p.matches(&full) || name.as_deref().is_some_and(|n| p.matches(n)),
                Err(_) => false,
            })
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
        match k.as_str() {
            "max-line-length" => raw.max_line_length = v.as_integer().map(|n| n as usize),
            "max-nesting-depth" => raw.max_nesting_depth = v.as_integer().map(|n| n as usize),
            "max-complexity" => raw.max_complexity = v.as_integer().map(|n| n as u32),
            "select" => raw.select = Some(string_array(v)?),
            "ignore" => raw.ignore = Some(string_array(v)?),
            "exclude" => raw.exclude = Some(string_array(v)?),
            other => return Err(ConfigError::UnknownKey(other.to_string())),
        }
    }
    Ok(raw)
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
    fn default_enables_all() {
        assert_eq!(Config::default().enabled.len(), 13);
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
        let cfg = Config::discover(&tmp).unwrap();
        assert!(cfg.enabled.len() <= 13);
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
}
