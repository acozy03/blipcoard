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
    #[serde(default)]
    pub capture: CaptureConfig,
    #[serde(default)]
    pub hosted: HostedConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedConfig {
    pub service_url: Option<String>,
    pub workspace_id: Option<String>,
    pub workspace_name: Option<String>,
    pub member_id: Option<String>,
    pub member_display_name: Option<String>,
    pub member_role: Option<String>,
    #[serde(default)]
    pub sticky_share_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub text: bool,
    #[serde(default = "default_true")]
    pub image: bool,
    #[serde(default = "default_true")]
    pub file_list: bool,
    #[serde(default = "default_true")]
    pub html: bool,
    #[serde(default = "default_true")]
    pub rtf: bool,
    #[serde(default = "default_true")]
    pub unknown: bool,
    #[serde(default = "default_max_image_bytes")]
    pub max_image_bytes: usize,
    #[serde(default = "default_true")]
    pub image_previews: bool,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            text: true,
            image: true,
            file_list: true,
            html: true,
            rtf: true,
            unknown: true,
            max_image_bytes: default_max_image_bytes(),
            image_previews: true,
        }
    }
}

impl CaptureConfig {
    pub fn text_enabled(&self) -> bool {
        self.enabled && self.text
    }

    pub fn image_enabled(&self) -> bool {
        self.enabled && self.image
    }

    pub fn file_list_enabled(&self) -> bool {
        self.enabled && self.file_list
    }

    pub fn html_enabled(&self) -> bool {
        self.enabled && self.html
    }

    pub fn rtf_enabled(&self) -> bool {
        self.enabled && self.rtf
    }

    pub fn unknown_enabled(&self) -> bool {
        self.enabled && self.unknown
    }
}

impl BlipConfig {
    pub fn load_or_create() -> Result<Self, ConfigError> {
        let config_path = config_file_path()?;
        let mut config = if config_path.exists() {
            let contents = fs::read_to_string(&config_path)?;
            toml::from_str(&contents)?
        } else {
            let config = Self::default_for_platform()?;
            config.persist(&config_path)?;
            config
        };

        if let Some(db_path) = env_override_database_path() {
            if let Some(parent) = db_path.parent() {
                fs::create_dir_all(parent)?;
            }
            config.database_path = db_path;
        }

        Ok(config)
    }

    pub fn default_for_platform() -> Result<Self, ConfigError> {
        let dirs = project_dirs()?;
        let data_dir = dirs.data_local_dir();
        fs::create_dir_all(data_dir)?;

        Ok(Self {
            database_path: data_dir.join("blipcoard.db"),
            capture: CaptureConfig::default(),
            hosted: HostedConfig::default(),
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

const fn default_true() -> bool {
    true
}

const fn default_max_image_bytes() -> usize {
    50 * 1024 * 1024
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_config_defaults_capture_policy() {
        let config: BlipConfig =
            toml::from_str("database_path = '/tmp/blipcoard.db'\n").expect("config should parse");

        assert!(config.capture.enabled);
        assert!(config.capture.image_enabled());
        assert!(config.capture.file_list_enabled());
        assert!(config.capture.html_enabled());
        assert!(config.capture.rtf_enabled());
        assert!(config.capture.unknown_enabled());
        assert!(config.capture.image_previews);
        assert_eq!(config.hosted, HostedConfig::default());
    }

    #[test]
    fn capture_global_disable_overrides_payload_flags() {
        let config: BlipConfig = toml::from_str(
            "
database_path = '/tmp/blipcoard.db'

[capture]
enabled = false
image = true
",
        )
        .expect("config should parse");

        assert!(!config.capture.image_enabled());
    }

    #[test]
    fn database_path_override_preserves_file_capture_policy() {
        let temp_dir =
            std::env::temp_dir().join(format!("blipcoard-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).expect("temp dir should create");
        let config_path = temp_dir.join("config.toml");
        std::fs::write(
            &config_path,
            "
database_path = '/tmp/original.db'

[capture]
enabled = false
",
        )
        .expect("config should write");

        let config: BlipConfig =
            toml::from_str(&std::fs::read_to_string(config_path).expect("config should read"))
                .expect("config should parse");

        assert!(!config.capture.enabled);
        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
