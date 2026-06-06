use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("unable to determine platform config directories")]
    MissingProjectDirs,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("toml parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("toml serialize error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlipConfig {
    pub database_path: PathBuf,
}

impl BlipConfig {
    pub fn load_or_create() -> Result<Self, ConfigError> {
        let config_path = config_file_path()?;

        if config_path.exists() {
            let contents = fs::read_to_string(&config_path)?;
            return Ok(toml::from_str(&contents)?);
        }

        let config = Self::default_for_platform()?;
        config.persist(&config_path)?;
        Ok(config)
    }

    pub fn default_for_platform() -> Result<Self, ConfigError> {
        let dirs = project_dirs()?;
        let data_dir = dirs.data_local_dir();
        fs::create_dir_all(data_dir)?;

        Ok(Self {
            database_path: data_dir.join("blipcoard.db"),
        })
    }

    pub fn persist(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized = toml::to_string_pretty(self)?;
        fs::write(path, serialized)?;
        Ok(())
    }
}

pub fn config_file_path() -> Result<PathBuf, ConfigError> {
    let dirs = project_dirs()?;
    fs::create_dir_all(dirs.config_dir())?;
    Ok(dirs.config_dir().join("config.toml"))
}

fn project_dirs() -> Result<ProjectDirs, ConfigError> {
    ProjectDirs::from("com", "acozy03", "blipcoard").ok_or(ConfigError::MissingProjectDirs)
}
