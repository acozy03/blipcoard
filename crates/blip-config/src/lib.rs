use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::env;
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
        if let Some(db_path) = env_override_database_path() {
            if let Some(parent) = db_path.parent() {
                fs::create_dir_all(parent)?;
            }

            return Ok(Self {
                database_path: db_path,
            });
        }

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

    pub fn daemon_socket_path(&self) -> Result<PathBuf, ConfigError> {
        if let Some(socket_path) = env_override_socket_path() {
            if let Some(parent) = socket_path.parent() {
                fs::create_dir_all(parent)?;
            }

            return Ok(socket_path);
        }

        if let Some(parent) = self.database_path.parent() {
            fs::create_dir_all(parent)?;
        }

        Ok(self.database_path.with_extension("sock"))
    }
}

pub fn config_file_path() -> Result<PathBuf, ConfigError> {
    if let Some(config_dir) = env_override_config_dir() {
        fs::create_dir_all(&config_dir)?;
        return Ok(config_dir.join("config.toml"));
    }

    let dirs = project_dirs()?;
    fs::create_dir_all(dirs.config_dir())?;
    Ok(dirs.config_dir().join("config.toml"))
}

fn env_override_config_dir() -> Option<PathBuf> {
    env::var_os("BLIPCOARD_CONFIG_DIR").map(PathBuf::from)
}

fn env_override_database_path() -> Option<PathBuf> {
    env::var_os("BLIPCOARD_DB_PATH").map(PathBuf::from)
}

fn env_override_socket_path() -> Option<PathBuf> {
    env::var_os("BLIPCOARD_SOCKET_PATH").map(PathBuf::from)
}

fn project_dirs() -> Result<ProjectDirs, ConfigError> {
    ProjectDirs::from("com", "acozy03", "blipcoard").ok_or(ConfigError::MissingProjectDirs)
}
