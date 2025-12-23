//! Logging configuration and initialization.
//!
//! This module provides structured logging with:
//! - Multiple presets (production, verbose, debug, trace, quiet)
//! - Per-target level overrides via CLI flags
//! - JSON output format for log aggregation
//! - Environment variable fallback (RUST_LOG)

use std::collections::HashMap;
use tracing::Level;
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

/// Log output format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LogFormat {
    #[default]
    Text,
    Json,
}

impl std::str::FromStr for LogFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" => Ok(LogFormat::Text),
            "json" => Ok(LogFormat::Json),
            _ => Err(format!("Invalid log format: '{}'. Use 'text' or 'json'.", s)),
        }
    }
}

/// Logging preset levels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LogPreset {
    /// Production: minimal logging, only important events
    #[default]
    Production,
    /// Verbose: more operational detail
    Verbose,
    /// Debug: detailed info for troubleshooting
    Debug,
    /// Trace: everything including high-frequency data
    Trace,
    /// Quiet: warnings and errors only
    Quiet,
}

/// Logging configuration built from CLI arguments.
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// Base preset to use
    pub preset: LogPreset,
    /// Per-target level overrides (e.g., "activity::state" -> DEBUG)
    pub overrides: HashMap<String, Level>,
    /// Output format
    pub format: LogFormat,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            preset: LogPreset::Production,
            overrides: HashMap::new(),
            format: LogFormat::Text,
        }
    }
}

impl LogConfig {
    /// Create a new LogConfig from CLI arguments.
    pub fn from_cli(
        verbose: bool,
        debug: bool,
        trace: bool,
        quiet: bool,
        log_overrides: Vec<String>,
        format: LogFormat,
    ) -> Self {
        // Determine preset from flags (last one wins if multiple specified)
        let preset = if quiet {
            LogPreset::Quiet
        } else if trace {
            LogPreset::Trace
        } else if debug {
            LogPreset::Debug
        } else if verbose {
            LogPreset::Verbose
        } else {
            LogPreset::Production
        };

        // Parse log overrides (format: "target=level" or "target::subtarget=level")
        let mut overrides = HashMap::new();
        for override_str in log_overrides {
            for part in override_str.split(',') {
                if let Some((target, level_str)) = part.split_once('=') {
                    let target = target.trim();
                    let level_str = level_str.trim();

                    // Normalize target: "activity" -> "clauset::activity"
                    let full_target = if target.starts_with("clauset::") || target == "tower_http" {
                        target.to_string()
                    } else {
                        format!("clauset::{}", target)
                    };

                    if let Ok(level) = parse_level(level_str) {
                        overrides.insert(full_target, level);
                    }
                }
            }
        }

        Self {
            preset,
            overrides,
            format,
        }
    }

    /// Build an EnvFilter from this configuration.
    pub fn build_filter(&self) -> EnvFilter {
        // Check for RUST_LOG environment variable first
        if let Ok(env_filter) = EnvFilter::try_from_default_env() {
            return env_filter;
        }

        // Build filter string from preset
        let mut directives: Vec<String> = match self.preset {
            LogPreset::Production => vec![
                "clauset::startup=info".into(),
                "clauset::api=info".into(),
                "clauset::ws=info".into(),
                "clauset::ws::ping=off".into(),
                "clauset::session=info".into(),
                "clauset::process=info".into(),
                "clauset::activity=warn".into(),
                "clauset::activity::state=off".into(),
                "clauset::activity::stats=off".into(),
                "clauset::parser=warn".into(),
                "clauset::events=info".into(),
                "clauset::hooks=info".into(),
                "tower_http=warn".into(),
            ],
            LogPreset::Verbose => vec![
                "clauset=info".into(),
                "clauset::activity=info".into(),
                "clauset::ws::ping=off".into(),
                "tower_http=info".into(),
            ],
            LogPreset::Debug => vec![
                "clauset=debug".into(),
                "clauset::ws::ping=off".into(),
                "tower_http=debug".into(),
            ],
            LogPreset::Trace => vec![
                "clauset=trace".into(),
                "tower_http=trace".into(),
            ],
            LogPreset::Quiet => vec![
                "clauset=warn".into(),
                "tower_http=error".into(),
            ],
        };

        // Apply overrides (they take precedence)
        for (target, level) in &self.overrides {
            directives.push(format!("{}={}", target, level_to_str(*level)));
        }

        let filter_str = directives.join(",");
        EnvFilter::try_new(&filter_str).unwrap_or_else(|_| EnvFilter::new("info"))
    }
}

/// Parse a level string (case-insensitive).
fn parse_level(s: &str) -> Result<Level, ()> {
    match s.to_lowercase().as_str() {
        "trace" => Ok(Level::TRACE),
        "debug" => Ok(Level::DEBUG),
        "info" => Ok(Level::INFO),
        "warn" | "warning" => Ok(Level::WARN),
        "error" => Ok(Level::ERROR),
        _ => Err(()),
    }
}

/// Convert a Level to its filter string representation.
fn level_to_str(level: Level) -> &'static str {
    match level {
        Level::TRACE => "trace",
        Level::DEBUG => "debug",
        Level::INFO => "info",
        Level::WARN => "warn",
        Level::ERROR => "error",
    }
}

/// Initialize the tracing subscriber with the given configuration.
pub fn init(config: &LogConfig) {
    let filter = config.build_filter();

    match config.format {
        LogFormat::Text => {
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    fmt::layer()
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_file(false)
                        .with_line_number(false)
                )
                .init();
        }
        LogFormat::Json => {
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    fmt::layer()
                        .json()
                        .with_target(true)
                        .with_span_events(FmtSpan::CLOSE)
                )
                .init();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_format_from_str() {
        assert_eq!("text".parse::<LogFormat>().unwrap(), LogFormat::Text);
        assert_eq!("json".parse::<LogFormat>().unwrap(), LogFormat::Json);
        assert_eq!("JSON".parse::<LogFormat>().unwrap(), LogFormat::Json);
        assert!("invalid".parse::<LogFormat>().is_err());
    }

    #[test]
    fn test_config_from_cli_preset_priority() {
        // Quiet should win
        let config = LogConfig::from_cli(true, true, true, true, vec![], LogFormat::Text);
        assert_eq!(config.preset, LogPreset::Quiet);

        // Trace wins over debug and verbose
        let config = LogConfig::from_cli(true, true, true, false, vec![], LogFormat::Text);
        assert_eq!(config.preset, LogPreset::Trace);

        // Debug wins over verbose
        let config = LogConfig::from_cli(true, true, false, false, vec![], LogFormat::Text);
        assert_eq!(config.preset, LogPreset::Debug);

        // Verbose alone
        let config = LogConfig::from_cli(true, false, false, false, vec![], LogFormat::Text);
        assert_eq!(config.preset, LogPreset::Verbose);

        // Default is production
        let config = LogConfig::from_cli(false, false, false, false, vec![], LogFormat::Text);
        assert_eq!(config.preset, LogPreset::Production);
    }

    #[test]
    fn test_config_overrides_parsing() {
        let config = LogConfig::from_cli(
            false, false, false, false,
            vec!["activity=debug".into(), "ws::ping=trace,session=info".into()],
            LogFormat::Text,
        );

        assert_eq!(config.overrides.get("clauset::activity"), Some(&Level::DEBUG));
        assert_eq!(config.overrides.get("clauset::ws::ping"), Some(&Level::TRACE));
        assert_eq!(config.overrides.get("clauset::session"), Some(&Level::INFO));
    }

    #[test]
    fn test_config_full_target_passthrough() {
        let config = LogConfig::from_cli(
            false, false, false, false,
            vec!["clauset::activity::state=debug".into(), "tower_http=trace".into()],
            LogFormat::Text,
        );

        assert_eq!(config.overrides.get("clauset::activity::state"), Some(&Level::DEBUG));
        assert_eq!(config.overrides.get("tower_http"), Some(&Level::TRACE));
    }
}
