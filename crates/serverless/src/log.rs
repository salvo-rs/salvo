//! Runtime logging configuration for serverless functions.
//!
//! Configuration is built at runtime; nothing is read at compile time.

use tracing_subscriber::EnvFilter;

/// Configuration for [`TracingLog`].
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// Maximum tracing level, for example `info` or `debug`.
    pub level: String,
    /// Extra per-target filter directives, for example `salvo=debug`.
    pub filter: String,
    /// Whether terminal output uses ANSI colors.
    pub ansi: bool,
    /// Whether events are formatted as JSON.
    pub json: bool,
    /// Whether thread ids and names are included in each event.
    pub performance: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".to_owned(),
            filter: String::new(),
            ansi: true,
            json: false,
            performance: false,
        }
    }
}

impl LogConfig {
    /// Creates a configuration with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the maximum tracing level.
    #[must_use]
    pub fn level(mut self, level: impl Into<String>) -> Self {
        self.level = level.into();
        self
    }

    /// Sets extra filter directives.
    #[must_use]
    pub fn filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = filter.into();
        self
    }

    /// Enables or disables ANSI colors.
    #[must_use]
    pub fn ansi(mut self, ansi: bool) -> Self {
        self.ansi = ansi;
        self
    }

    /// Enables or disables JSON output.
    #[must_use]
    pub fn json(mut self, json: bool) -> Self {
        self.json = json;
        self
    }

    /// Enables or disables thread ids and names in events.
    #[must_use]
    pub fn performance(mut self, performance: bool) -> Self {
        self.performance = performance;
        self
    }
}

/// Error returned by [`TracingLog::init`].
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct LogError(String);

/// Builds and installs a global `tracing` subscriber at runtime.
#[derive(Debug, Default)]
pub struct TracingLog;

impl TracingLog {
    /// Installs a global subscriber built from `config`.
    ///
    /// Returns an error if the filter directives are invalid or a global
    /// subscriber was already installed.
    ///
    /// # Errors
    ///
    /// Returns [`LogError`] when the filter cannot be parsed or the global
    /// subscriber cannot be installed.
    pub fn init(config: &LogConfig) -> Result<(), LogError> {
        let directives = if config.filter.is_empty() {
            config.level.clone()
        } else {
            format!("{},{}", config.level, config.filter)
        };
        let filter = EnvFilter::try_new(directives).map_err(|err| LogError(err.to_string()))?;
        let builder = tracing_subscriber::fmt()
            .with_ansi(config.ansi)
            .with_thread_ids(config.performance)
            .with_thread_names(config.performance);
        let result = if config.json {
            builder.json().with_env_filter(filter).try_init()
        } else {
            builder.with_env_filter(filter).try_init()
        };
        result.map_err(|err| LogError(err.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_and_builder() {
        let config = LogConfig::new()
            .level("debug")
            .filter("salvo=trace")
            .ansi(false)
            .json(true)
            .performance(true);
        assert_eq!(config.level, "debug");
        assert_eq!(config.filter, "salvo=trace");
        assert!(!config.ansi);
        assert!(config.json);
        assert!(config.performance);
        assert_eq!(LogConfig::default().level, "info");
    }

    #[test]
    fn init_rejects_invalid_filter() {
        // Fails while parsing the filter, before any global subscriber is
        // installed, so this is safe to run alongside other tests.
        let config = LogConfig::new().filter("salvo=not_a_valid_level");
        assert!(TracingLog::init(&config).is_err());
    }
}
