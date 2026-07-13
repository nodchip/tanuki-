use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("configuration I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("configuration TOML is invalid: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("{0}")]
    Invalid(String),
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct WorkerConfig {
    pub engine_count: usize,
    pub threads_per_engine: usize,
    pub vulnerability_black: usize,
    pub vulnerability_white: usize,
    pub general: usize,
}

impl WorkerConfig {
    pub fn roles(&self) -> Vec<&'static str> {
        let mut roles = Vec::with_capacity(self.engine_count);
        roles.extend(std::iter::repeat_n(
            "vulnerability_black",
            self.vulnerability_black,
        ));
        roles.extend(std::iter::repeat_n(
            "vulnerability_white",
            self.vulnerability_white,
        ));
        roles.extend(std::iter::repeat_n("general", self.general));
        roles
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.engine_count < 1 {
            return invalid("engine_count must be at least 1");
        }
        if self.threads_per_engine < 1 {
            return invalid("threads_per_engine must be at least 1");
        }
        let assigned = self.vulnerability_black + self.vulnerability_white + self.general;
        if assigned != self.engine_count {
            return invalid(format!(
                "engine_count must equal vulnerability_black + vulnerability_white + general ({assigned})"
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct CorpusConfig {
    pub enabled: bool,
    pub max_concurrent_searches: usize,
    pub general_pool_node_share: f64,
    #[serde(default = "default_saturation_window")]
    pub saturation_window: usize,
    #[serde(default = "default_wcsc_weight")]
    pub wcsc_weight: u64,
    #[serde(default = "default_denryu_weight")]
    pub denryu_weight: u64,
    #[serde(default = "default_floodgate_weight")]
    pub floodgate_weight: u64,
    #[serde(default = "default_rating_medium_games")]
    pub rating_medium_games: u64,
    #[serde(default = "default_rating_high_games")]
    pub rating_high_games: u64,
    #[serde(default = "default_rating_min_component_size")]
    pub rating_min_component_size: u64,
}

impl CorpusConfig {
    fn validate(&self, general_workers: usize) -> Result<(), ConfigError> {
        if !(0.0..=1.0).contains(&self.general_pool_node_share) {
            return invalid("general_pool_node_share must be between 0 and 1");
        }
        if self.saturation_window < 1 {
            return invalid("saturation_window must be positive");
        }
        if self.wcsc_weight + self.denryu_weight + self.floodgate_weight == 0 {
            return invalid("at least one site weight must be positive");
        }
        if !(0 < self.rating_medium_games && self.rating_medium_games < self.rating_high_games) {
            return invalid("rating game thresholds must be increasing");
        }
        if self.rating_min_component_size < 1 {
            return invalid("rating_min_component_size must be positive");
        }
        if self.enabled {
            if general_workers < 1 {
                return invalid("corpus requires at least one general worker");
            }
            if self.max_concurrent_searches < 1 || self.max_concurrent_searches > general_workers {
                return invalid(
                    "max_concurrent_searches must be between 1 and the number of general workers",
                );
            }
        } else if self.max_concurrent_searches != 0 {
            return invalid("disabled corpus must use max_concurrent_searches = 0");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct RuntimeConfig {
    pub state_dir: PathBuf,
    pub save_interval_sec: f64,
    pub backup_count: usize,
    pub heartbeat_timeout_sec: f64,
    pub usi_stop_timeout_sec: f64,
}

impl RuntimeConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.save_interval_sec <= 0.0 {
            return invalid("save_interval_sec must be positive");
        }
        if self.heartbeat_timeout_sec <= 0.0 {
            return invalid("heartbeat_timeout_sec must be positive");
        }
        if self.usi_stop_timeout_sec <= 0.0 {
            return invalid("usi_stop_timeout_sec must be positive");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ExtensionConfig {
    pub workers: WorkerConfig,
    pub corpus: CorpusConfig,
    pub runtime: RuntimeConfig,
}

impl ExtensionConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = fs::read_to_string(path)?;
        Self::from_toml(&text, path)
    }

    pub fn from_toml(text: &str, config_path: &Path) -> Result<Self, ConfigError> {
        let mut config: Self = toml::from_str(text)?;
        if !config.runtime.state_dir.is_absolute() {
            let parent = config_path.parent().unwrap_or_else(|| Path::new("."));
            config.runtime.state_dir = parent.join(&config.runtime.state_dir);
        }
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        self.workers.validate()?;
        self.corpus.validate(self.workers.general)?;
        self.runtime.validate()
    }
}

fn invalid<T>(message: impl Into<String>) -> Result<T, ConfigError> {
    Err(ConfigError::Invalid(message.into()))
}

const fn default_saturation_window() -> usize {
    100
}
const fn default_wcsc_weight() -> u64 {
    40
}
const fn default_denryu_weight() -> u64 {
    40
}
const fn default_floodgate_weight() -> u64 {
    20
}
const fn default_rating_medium_games() -> u64 {
    15
}
const fn default_rating_high_games() -> u64 {
    50
}
const fn default_rating_min_component_size() -> u64 {
    10
}
