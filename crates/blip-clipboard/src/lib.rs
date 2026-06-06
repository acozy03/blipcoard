use std::fmt;
use std::time::Duration;
use thiserror::Error;

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardPlatform {
    Linux,
    MacOs,
    Windows,
    Unknown,
}

impl fmt::Display for ClipboardPlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Linux => "linux",
            Self::MacOs => "macos",
            Self::Windows => "windows",
            Self::Unknown => "unknown",
        };

        f.write_str(name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardWatcherConfig {
    pub poll_interval: Duration,
}

impl Default for ClipboardWatcherConfig {
    fn default() -> Self {
        Self {
            poll_interval: DEFAULT_POLL_INTERVAL,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardEvent {
    pub text: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ClipboardError {
    #[error(
        "clipboard watcher is not available on {platform}: {reason}. \
         blip-clipboard keeps platform integration isolated behind this boundary"
    )]
    Unavailable {
        platform: ClipboardPlatform,
        reason: &'static str,
    },
}

pub trait ClipboardReader {
    fn read_text(&mut self) -> Result<Option<String>, ClipboardError>;
}

pub trait ClipboardWatcher {
    fn poll_next(&mut self) -> Result<Option<ClipboardEvent>, ClipboardError>;
    fn poll_interval(&self) -> Duration;
}

#[derive(Debug)]
pub struct PollingClipboardWatcher<R> {
    reader: R,
    poll_interval: Duration,
    last_text: Option<String>,
}

impl<R> PollingClipboardWatcher<R>
where
    R: ClipboardReader,
{
    pub fn new(reader: R, config: ClipboardWatcherConfig) -> Self {
        Self {
            reader,
            poll_interval: config.poll_interval,
            last_text: None,
        }
    }
}

impl<R> ClipboardWatcher for PollingClipboardWatcher<R>
where
    R: ClipboardReader,
{
    fn poll_next(&mut self) -> Result<Option<ClipboardEvent>, ClipboardError> {
        let Some(text) = self.reader.read_text()? else {
            return Ok(None);
        };

        if self.last_text.as_ref() == Some(&text) {
            return Ok(None);
        }

        self.last_text = Some(text.clone());
        Ok(Some(ClipboardEvent { text }))
    }

    fn poll_interval(&self) -> Duration {
        self.poll_interval
    }
}

pub fn current_platform() -> ClipboardPlatform {
    if cfg!(target_os = "linux") {
        ClipboardPlatform::Linux
    } else if cfg!(target_os = "macos") {
        ClipboardPlatform::MacOs
    } else if cfg!(target_os = "windows") {
        ClipboardPlatform::Windows
    } else {
        ClipboardPlatform::Unknown
    }
}

pub fn watcher_strategy_hint() -> &'static str {
    match current_platform() {
        ClipboardPlatform::Linux => "linux clipboard watcher pending: x11/wayland abstraction",
        ClipboardPlatform::MacOs => "macos clipboard watcher pending: pasteboard integration",
        ClipboardPlatform::Windows => "windows clipboard watcher pending: win32 listener",
        ClipboardPlatform::Unknown => "unsupported platform",
    }
}

pub fn system_watcher(
    config: ClipboardWatcherConfig,
) -> Result<PollingClipboardWatcher<PlatformClipboardReader>, ClipboardError> {
    Ok(PollingClipboardWatcher::new(
        PlatformClipboardReader::new()?,
        config,
    ))
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{ClipboardError, ClipboardPlatform};

    #[derive(Debug, Default)]
    pub struct PlatformClipboardReader;

    impl PlatformClipboardReader {
        pub fn new() -> Result<Self, ClipboardError> {
            Err(ClipboardError::Unavailable {
                platform: ClipboardPlatform::Linux,
                reason: "x11/wayland clipboard reader is not implemented yet",
            })
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{ClipboardError, ClipboardPlatform};

    #[derive(Debug, Default)]
    pub struct PlatformClipboardReader;

    impl PlatformClipboardReader {
        pub fn new() -> Result<Self, ClipboardError> {
            Err(ClipboardError::Unavailable {
                platform: ClipboardPlatform::MacOs,
                reason: "pasteboard clipboard reader is not implemented yet",
            })
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{ClipboardError, ClipboardPlatform};

    #[derive(Debug, Default)]
    pub struct PlatformClipboardReader;

    impl PlatformClipboardReader {
        pub fn new() -> Result<Self, ClipboardError> {
            Err(ClipboardError::Unavailable {
                platform: ClipboardPlatform::Windows,
                reason: "win32 clipboard reader is not implemented yet",
            })
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod platform {
    use super::{ClipboardError, ClipboardPlatform};

    #[derive(Debug, Default)]
    pub struct PlatformClipboardReader;

    impl PlatformClipboardReader {
        pub fn new() -> Result<Self, ClipboardError> {
            Err(ClipboardError::Unavailable {
                platform: ClipboardPlatform::Unknown,
                reason: "this target does not have a supported clipboard backend",
            })
        }
    }
}

pub use platform::PlatformClipboardReader;

impl ClipboardReader for PlatformClipboardReader {
    fn read_text(&mut self) -> Result<Option<String>, ClipboardError> {
        Err(ClipboardError::Unavailable {
            platform: current_platform(),
            reason: watcher_strategy_hint(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct StubClipboardReader {
        reads: Vec<Option<&'static str>>,
        index: usize,
    }

    impl ClipboardReader for StubClipboardReader {
        fn read_text(&mut self) -> Result<Option<String>, ClipboardError> {
            let value = self.reads.get(self.index).copied().flatten();
            self.index += 1;
            Ok(value.map(std::string::ToString::to_string))
        }
    }

    #[test]
    fn polling_watcher_emits_only_when_text_changes() {
        let reader = StubClipboardReader {
            reads: vec![Some("first"), Some("first"), Some("second"), None],
            index: 0,
        };
        let mut watcher = PollingClipboardWatcher::new(reader, ClipboardWatcherConfig::default());

        assert_eq!(
            watcher.poll_next().expect("poll should succeed"),
            Some(ClipboardEvent {
                text: "first".to_string(),
            })
        );
        assert_eq!(watcher.poll_next().expect("poll should succeed"), None);
        assert_eq!(
            watcher.poll_next().expect("poll should succeed"),
            Some(ClipboardEvent {
                text: "second".to_string(),
            })
        );
        assert_eq!(watcher.poll_next().expect("poll should succeed"), None);
    }

    #[test]
    fn polling_watcher_preserves_configured_interval() {
        let watcher = PollingClipboardWatcher::new(
            StubClipboardReader {
                reads: vec![None],
                index: 0,
            },
            ClipboardWatcherConfig {
                poll_interval: Duration::from_secs(2),
            },
        );

        assert_eq!(watcher.poll_interval(), Duration::from_secs(2));
    }

    #[test]
    fn system_watcher_returns_clear_platform_error() {
        let error = system_watcher(ClipboardWatcherConfig::default())
            .expect_err("platform backend should remain explicit until implemented");

        match error {
            ClipboardError::Unavailable { platform, reason } => {
                assert_eq!(platform, current_platform());
                assert!(!reason.is_empty());
            }
        }
    }
}
